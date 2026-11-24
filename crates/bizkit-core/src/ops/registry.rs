//! The operation registry: registration checks, type erasure and the access
//! check that HTTP and MCP share.

use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Context as _, bail};
use axum::http::StatusCode;
use serde_json::Value;

use super::{
    Access, Method, NoContent, NoInput, OpCtx, Operation, Principal, ScopeAreas, Success,
    authorize_workspace,
};
use crate::app::Platform;
use crate::error::ApiError;
use crate::validate::{Validate as _, from_json};

/// What the registry knows about an operation, for docs and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpMeta {
    /// The operation ID.
    pub id: &'static str,
    /// The HTTP method.
    pub method: Method,
    /// The path under `/api/v1`.
    pub path: &'static str,
    /// Who may call it.
    pub access: Access,
    /// The success status.
    pub success: Success,
    /// The one-line summary.
    pub summary: &'static str,
    /// OpenAPI tags.
    pub tags: &'static [&'static str],
    /// The `{...}` names in the path, in order.
    pub path_params: Vec<&'static str>,
    /// Extra documented error statuses.
    pub errors: &'static [StatusCode],
    /// True when the input is [`NoInput`].
    pub no_input: bool,
}

/// How a path or query string value coerces into JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FieldKind {
    String,
    Integer,
    Number,
    Boolean,
    Array(Box<Self>),
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

trait ErasedOp: Send + Sync + fmt::Debug {
    fn call_json(&self, cx: OpCtx, input: Value) -> BoxFuture<Result<Value, ApiError>>;
}

struct Erased<O>(Arc<O>);

impl<O: Operation> fmt::Debug for Erased<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Erased").field(&O::ID).finish()
    }
}

impl<O: Operation> ErasedOp for Erased<O> {
    fn call_json(&self, cx: OpCtx, input: Value) -> BoxFuture<Result<Value, ApiError>> {
        let op = Arc::clone(&self.0);
        Box::pin(async move {
            let input: O::Input = from_json(input)?;
            input.validate()?;
            let output = op.call(cx, input).await?;
            serde_json::to_value(output)
                .context("serializing the output")
                .map_err(ApiError::Internal)
        })
    }
}

/// A registered operation.
#[derive(Debug)]
pub(crate) struct RegisteredOp {
    pub(crate) meta: OpMeta,
    /// The `Input` object schema, with refs into the registry's components.
    pub(crate) input_schema: serde_json::Map<String, Value>,
    /// How each top-level input field coerces from a string.
    pub(crate) field_kinds: BTreeMap<String, FieldKind>,
    /// The output component name; `None` for [`NoContent`].
    pub(crate) output: Option<String>,
    handler: Arc<dyn ErasedOp>,
}

/// Every operation, plus the scope areas and schema components they use.
#[derive(Debug, Default)]
pub struct Registry {
    ops: BTreeMap<&'static str, RegisteredOp>,
    areas: ScopeAreas,
    components: BTreeMap<String, Value>,
}

impl Registry {
    /// Adds `op`, rejecting anything the router, spec or access check can't
    /// handle.
    ///
    /// # Errors
    ///
    /// When the ID or method and path are taken, a path param conflicts with another route's, a workspace path lacks `{workspace_id}`, a path param isn't an input field, the scope area is undeclared, `NoContent` is mismatched, or a schema component name is taken by a different shape.
    pub fn add<O: Operation>(&mut self, op: O) -> anyhow::Result<()> {
        self.check::<O>().with_context(|| format!("registering operation {}", O::ID))?;
        let mut components = Vec::new();
        <O::Input as utoipa::ToSchema>::schemas(&mut components);
        <O::Output as utoipa::ToSchema>::schemas(&mut components);
        let is_no_content = TypeId::of::<O::Output>() == TypeId::of::<NoContent>();
        let output = if is_no_content {
            None
        } else {
            let name = <O::Output as utoipa::ToSchema>::name().into_owned();
            components.push((name.clone(), <O::Output as utoipa::PartialSchema>::schema()));
            Some(name)
        };
        let mut new_components = BTreeMap::new();
        for (name, schema) in components {
            let schema = serde_json::to_value(schema).context("serializing a schema")?;
            self.check_component(&name, &schema)
                .with_context(|| format!("registering operation {}", O::ID))?;
            new_components.insert(name, schema);
        }
        let Value::Object(input_schema) =
            serde_json::to_value(<O::Input as utoipa::PartialSchema>::schema())?
        else {
            bail!("operation {}: input schema must be an object", O::ID);
        };
        let field_kinds = field_kinds(&input_schema, |name| {
            new_components.get(name).or_else(|| self.components.get(name))
        });
        let path_params = path_params(O::PATH);
        let properties = input_schema.get("properties").and_then(Value::as_object);
        for param in &path_params {
            if !properties.is_some_and(|props| props.contains_key(*param)) {
                bail!("operation {}: path param {{{param}}} is not a field of the input", O::ID);
            }
        }
        self.components.extend(new_components);
        let meta = OpMeta {
            id: O::ID,
            method: O::METHOD,
            path: O::PATH,
            access: O::ACCESS,
            success: O::SUCCESS,
            summary: O::SUMMARY,
            tags: O::TAGS,
            path_params,
            errors: O::ERRORS,
            no_input: TypeId::of::<O::Input>() == TypeId::of::<NoInput>(),
        };
        let handler: Arc<dyn ErasedOp> = Arc::new(Erased(Arc::new(op)));
        self.ops.insert(O::ID, RegisteredOp { meta, input_schema, field_kinds, output, handler });
        Ok(())
    }

    fn check<O: Operation>(&self) -> anyhow::Result<()> {
        if self.ops.contains_key(O::ID) {
            bail!("duplicate operation ID");
        }
        if !O::PATH.starts_with('/') {
            bail!("path {} must start with /", O::PATH);
        }
        for existing in self.ops.values() {
            if existing.meta.method == O::METHOD && existing.meta.path == O::PATH {
                bail!(
                    "{} {} is already operation {}",
                    O::METHOD.as_lower(),
                    O::PATH,
                    existing.meta.id
                );
            }
            if let Some((ours, theirs)) = param_conflict(O::PATH, existing.meta.path) {
                bail!(
                    "path {} names a param {{{ours}}} where {} (operation {}) names {{{theirs}}}",
                    O::PATH,
                    existing.meta.path,
                    existing.meta.id
                );
            }
        }
        if let Access::Workspace(scope) = &O::ACCESS {
            if !path_params(O::PATH).contains(&"workspace_id") {
                bail!("workspace operations need {{workspace_id}} in the path");
            }
            if !self.areas.contains(scope.area()) {
                bail!("scope area {} is not declared", scope.area());
            }
        }
        let no_content_output = TypeId::of::<O::Output>() == TypeId::of::<NoContent>();
        if (O::SUCCESS == Success::NoContent) != no_content_output {
            bail!("Success::NoContent and the NoContent output go together");
        }
        Ok(())
    }

    fn check_component(&self, name: &str, schema: &Value) -> anyhow::Result<()> {
        match self.components.get(name) {
            Some(existing) if existing != schema => {
                bail!("schema component {name} is already registered with a different shape")
            }
            _ => Ok(()),
        }
    }

    pub(crate) fn declare_area(
        &mut self,
        area: &'static str,
        description: &'static str,
    ) -> anyhow::Result<()> {
        if !self.areas.insert(area, description) {
            bail!("scope area {area} is already declared");
        }
        Ok(())
    }

    /// The declared scope areas.
    #[must_use]
    pub const fn areas(&self) -> &ScopeAreas {
        &self.areas
    }

    /// Every operation, sorted by ID.
    pub fn ops(&self) -> impl Iterator<Item = &OpMeta> {
        self.ops.values().map(|op| &op.meta)
    }

    pub(crate) fn registered(&self) -> impl Iterator<Item = &RegisteredOp> {
        self.ops.values()
    }

    pub(crate) fn get(&self, id: &str) -> Option<&RegisteredOp> {
        self.ops.get(id)
    }

    pub(crate) const fn components(&self) -> &BTreeMap<String, Value> {
        &self.components
    }

    /// Steps 1-3 of the access check: authentication, workspace membership and
    /// scope. `workspace_id` is the raw `{workspace_id}` value, if any.
    pub(crate) async fn authorize(
        op: &RegisteredOp,
        platform: &Platform,
        principal: Option<Principal>,
        workspace_id: Option<&str>,
        request_id: String,
    ) -> Result<OpCtx, ApiError> {
        let workspace = match &op.meta.access {
            Access::Public => None,
            Access::User => match principal {
                Some(Principal::User(_)) => None,
                _ => return Err(ApiError::Unauthenticated),
            },
            Access::Workspace(scope) => {
                // An empty ID never parses, so a missing one still gives 401
                // before 404.
                let workspace_id = workspace_id.unwrap_or_default();
                Some(authorize_workspace(platform, principal.as_ref(), workspace_id, scope).await?)
            }
        };
        Ok(OpCtx::new(platform.clone(), principal, workspace, request_id))
    }

    /// Steps 4-5: deserialize, validate and call.
    pub(crate) async fn invoke(
        op: &RegisteredOp,
        cx: OpCtx,
        input: Value,
    ) -> Result<Value, ApiError> {
        op.handler.call_json(cx, input).await
    }
}

/// The `{...}` names in `path`, in order.
pub(crate) fn path_params(path: &'static str) -> Vec<&'static str> {
    path.split('/').filter_map(|seg| seg.strip_prefix('{')?.strip_suffix('}')).collect()
}

/// Axum panics when two routes name a param differently at the same position
/// under the same prefix (`/w/{workspace_id}/x` beside `/w/{id}/y`). Returns
/// the two names when `a` and `b` would.
fn param_conflict(a: &'static str, b: &'static str) -> Option<(&'static str, &'static str)> {
    let param = |seg: &'static str| seg.strip_prefix('{').and_then(|s| s.strip_suffix('}'));
    for (x, y) in a.split('/').zip(b.split('/')) {
        match (param(x), param(y)) {
            (Some(p), Some(q)) if p != q => return Some((p, q)),
            (Some(_), Some(_)) => {}
            (None, None) if x == y => {}
            _ => return None,
        }
    }
    None
}

fn field_kinds<'a>(
    schema: &serde_json::Map<String, Value>,
    lookup: impl Fn(&str) -> Option<&'a Value>,
) -> BTreeMap<String, FieldKind> {
    let Some(props) = schema.get("properties").and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    props.iter().map(|(name, prop)| (name.clone(), kind_of(prop, &lookup, 0))).collect()
}

fn kind_of<'a>(
    schema: &Value,
    lookup: &impl Fn(&str) -> Option<&'a Value>,
    depth: u8,
) -> FieldKind {
    if depth > 8 {
        return FieldKind::String;
    }
    if let Some(target) = schema.get("$ref").and_then(Value::as_str) {
        let name = target.rsplit('/').next().unwrap_or(target);
        return lookup(name)
            .map_or(FieldKind::String, |resolved| kind_of(resolved, lookup, depth + 1));
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(variants) = schema.get(key).and_then(Value::as_array) {
            let non_null =
                variants.iter().find(|v| v.get("type").and_then(Value::as_str) != Some("null"));
            return non_null.map_or(FieldKind::String, |v| kind_of(v, lookup, depth + 1));
        }
    }
    let types: BTreeSet<&str> = match schema.get("type") {
        Some(Value::String(t)) => BTreeSet::from([t.as_str()]),
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ => BTreeSet::new(),
    };
    if types.contains("array") {
        let items = schema
            .get("items")
            .map_or(FieldKind::String, |items| kind_of(items, lookup, depth + 1));
        FieldKind::Array(Box::new(items))
    } else if types.contains("integer") {
        FieldKind::Integer
    } else if types.contains("number") {
        FieldKind::Number
    } else if types.contains("boolean") {
        FieldKind::Boolean
    } else {
        FieldKind::String
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use utoipa::ToSchema;

    use super::*;
    use crate::ops::{Level, Scope};
    use crate::validate::Validate;

    #[derive(Debug, Deserialize, ToSchema)]
    #[expect(dead_code, reason = "fields exist for their schema")]
    struct In {
        workspace_id: String,
        limit: Option<u32>,
        tags: Vec<String>,
        flag: bool,
    }
    impl Validate for In {}

    #[derive(Debug, Serialize, ToSchema)]
    struct Out {
        ok: bool,
    }

    macro_rules! op {
        ($name:ident, $id:literal, $method:expr, $path:literal, $access:expr) => {
            op!($name, $id, $method, $path, $access, In, Out, Success::Ok);
        };
        ($name:ident, $id:literal, $method:expr, $path:literal, $access:expr, $in:ty, $out:ty, $success:expr) => {
            struct $name;
            impl Operation for $name {
                type Input = $in;
                type Output = $out;
                const ID: &'static str = $id;
                const METHOD: Method = $method;
                const PATH: &'static str = $path;
                const ACCESS: Access = $access;
                const SUMMARY: &'static str = "test";
                const SUCCESS: Success = $success;
                fn call(
                    &self,
                    _cx: OpCtx,
                    _input: Self::Input,
                ) -> impl Future<Output = Result<Self::Output, ApiError>> + Send {
                    std::future::ready(Err(ApiError::NotFound))
                }
            }
        };
    }

    const READ: Access = Access::Workspace(Scope::new("projects", Level::Read));

    op!(List, "things.list", Method::Get, "/workspaces/{workspace_id}/things", READ);
    op!(Again, "things.list", Method::Post, "/workspaces/{workspace_id}/other", READ);
    op!(SameRoute, "things.list2", Method::Get, "/workspaces/{workspace_id}/things", READ);
    op!(Renamed, "things.get", Method::Get, "/workspaces/{id}/things/x", READ);
    op!(NoWorkspace, "things.top", Method::Get, "/things", READ);
    op!(
        MissingParam,
        "things.one",
        Method::Get,
        "/workspaces/{workspace_id}/things/{thing_id}",
        READ
    );
    op!(
        UnknownArea,
        "things.area",
        Method::Get,
        "/workspaces/{workspace_id}/area",
        Access::Workspace(Scope::new("forms", Level::Read))
    );
    op!(
        BadNoContent,
        "things.del",
        Method::Delete,
        "/workspaces/{workspace_id}/del",
        READ,
        In,
        Out,
        Success::NoContent
    );
    op!(
        NoContentOut,
        "things.del2",
        Method::Delete,
        "/workspaces/{workspace_id}/del2",
        READ,
        In,
        NoContent,
        Success::Ok
    );

    fn registry() -> Registry {
        let mut registry = Registry::default();
        registry.declare_area("projects", "Projects").unwrap();
        registry.add(List).unwrap();
        registry
    }

    fn rejects<O: Operation>(op: O) -> String {
        format!("{:#}", registry().add(op).unwrap_err())
    }

    #[test]
    fn rejects_bad_registrations() {
        insta::assert_snapshot!(rejects(Again), @"registering operation things.list: duplicate operation ID");
        insta::assert_snapshot!(rejects(SameRoute), @"registering operation things.list2: get /workspaces/{workspace_id}/things is already operation things.list");
        insta::assert_snapshot!(rejects(Renamed), @"registering operation things.get: path /workspaces/{id}/things/x names a param {id} where /workspaces/{workspace_id}/things (operation things.list) names {workspace_id}");
        insta::assert_snapshot!(rejects(NoWorkspace), @"registering operation things.top: workspace operations need {workspace_id} in the path");
        insta::assert_snapshot!(rejects(MissingParam), @"operation things.one: path param {thing_id} is not a field of the input");
        insta::assert_snapshot!(rejects(UnknownArea), @"registering operation things.area: scope area forms is not declared");
        insta::assert_snapshot!(rejects(BadNoContent), @"registering operation things.del: Success::NoContent and the NoContent output go together");
        insta::assert_snapshot!(rejects(NoContentOut), @"registering operation things.del2: Success::NoContent and the NoContent output go together");
    }

    #[test]
    fn records_field_kinds() {
        let registry = registry();
        let kinds = &registry.get("things.list").unwrap().field_kinds;
        assert_eq!(kinds["limit"], FieldKind::Integer, "Option<u32>");
        assert_eq!(kinds["tags"], FieldKind::Array(Box::new(FieldKind::String)), "Vec<String>");
        assert_eq!(kinds["flag"], FieldKind::Boolean, "bool");
        assert_eq!(kinds["workspace_id"], FieldKind::String, "String");
    }

    #[test]
    fn param_conflicts() {
        assert_eq!(param_conflict("/a/{x}/b", "/a/{y}"), Some(("x", "y")), "same position");
        assert_eq!(param_conflict("/a/{x}/b", "/a/{x}/c"), None, "same name");
        assert_eq!(param_conflict("/a/{x}", "/b/{y}"), None, "different prefix");
        assert_eq!(param_conflict("/a/me", "/a/{y}"), None, "static beside a param");
    }
}
