//! Typed IDs. Each one is a `UUIDv7`, stored as hyphenated `TEXT` so rows stay
//! readable in the `sqlite3` shell.

/// Declares a `UUIDv7` ID newtype with database, serde and schema support.
///
/// Modules use it for their own IDs:
///
/// ```
/// bizkit_core::id_type!(FormId, "The ID of a form.");
/// let id = FormId::new();
/// assert_eq!(id.to_string().len(), 36);
/// ```
#[macro_export]
macro_rules! id_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
        )]
        pub struct $name(pub $crate::__private::uuid::Uuid);

        impl $crate::__private::serde::Serialize for $name {
            fn serialize<S: $crate::__private::serde::Serializer>(
                &self,
                serializer: S,
            ) -> ::std::result::Result<S::Ok, S::Error> {
                $crate::__private::serde::Serialize::serialize(&self.0, serializer)
            }
        }

        impl<'de> $crate::__private::serde::Deserialize<'de> for $name {
            fn deserialize<D: $crate::__private::serde::Deserializer<'de>>(
                deserializer: D,
            ) -> ::std::result::Result<Self, D::Error> {
                <$crate::__private::uuid::Uuid as $crate::__private::serde::Deserialize<'de>>::deserialize(deserializer).map(Self)
            }
        }

        impl $name {
            /// A new, time-ordered ID.
            #[must_use]
            pub fn new() -> Self {
                Self($crate::__private::uuid::Uuid::now_v7())
            }
        }

        impl ::std::default::Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                ::std::fmt::Display::fmt(&self.0.hyphenated(), f)
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::__private::uuid::Error;

            fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
                $crate::__private::uuid::Uuid::parse_str(s).map(Self)
            }
        }

        impl $crate::__private::sqlx::Type<$crate::__private::sqlx::Sqlite> for $name {
            fn type_info() -> <$crate::__private::sqlx::Sqlite as $crate::__private::sqlx::Database>::TypeInfo {
                <$crate::__private::uuid::fmt::Hyphenated as $crate::__private::sqlx::Type<$crate::__private::sqlx::Sqlite>>::type_info()
            }

            fn compatible(ty: &<$crate::__private::sqlx::Sqlite as $crate::__private::sqlx::Database>::TypeInfo) -> bool {
                <$crate::__private::uuid::fmt::Hyphenated as $crate::__private::sqlx::Type<$crate::__private::sqlx::Sqlite>>::compatible(ty)
            }
        }

        impl<'q> $crate::__private::sqlx::Encode<'q, $crate::__private::sqlx::Sqlite> for $name {
            fn encode_by_ref(
                &self,
                buf: &mut <$crate::__private::sqlx::Sqlite as $crate::__private::sqlx::Database>::ArgumentBuffer,
            ) -> ::std::result::Result<$crate::__private::sqlx::encode::IsNull, $crate::__private::sqlx::error::BoxDynError> {
                $crate::__private::sqlx::Encode::<'q, $crate::__private::sqlx::Sqlite>::encode(self.0.hyphenated(), buf)
            }
        }

        impl<'r> $crate::__private::sqlx::Decode<'r, $crate::__private::sqlx::Sqlite> for $name {
            fn decode(
                value: <$crate::__private::sqlx::Sqlite as $crate::__private::sqlx::Database>::ValueRef<'r>,
            ) -> ::std::result::Result<Self, $crate::__private::sqlx::error::BoxDynError> {
                let id = <$crate::__private::uuid::fmt::Hyphenated as $crate::__private::sqlx::Decode<'r, $crate::__private::sqlx::Sqlite>>::decode(
                    value,
                )?;
                Ok(Self(id.into_uuid()))
            }
        }

        impl $crate::__private::utoipa::PartialSchema for $name {
            fn schema() -> $crate::__private::utoipa::openapi::RefOr<$crate::__private::utoipa::openapi::Schema> {
                $crate::__private::utoipa::openapi::schema::ObjectBuilder::new()
                    .schema_type($crate::__private::utoipa::openapi::schema::Type::String)
                    .format(Some($crate::__private::utoipa::openapi::schema::SchemaFormat::KnownFormat(
                        $crate::__private::utoipa::openapi::schema::KnownFormat::Uuid,
                    )))
                    .into()
            }
        }

        impl $crate::__private::utoipa::ToSchema for $name {}
    };
}

id_type!(UserId, "The ID of a user account.");
id_type!(WorkspaceId, "The ID of a workspace, the unit of tenancy.");
id_type!(ProjectId, "The ID of a project inside a workspace.");
id_type!(ApiKeyId, "The ID of an API key.");
id_type!(InvitationId, "The ID of a workspace invitation.");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_hyphenated() {
        let id: UserId = "0190a5d0-0000-7000-8000-000000000001".parse().unwrap();
        assert_eq!(id.to_string(), "0190a5d0-0000-7000-8000-000000000001", "display");
    }

    #[test]
    fn serializes_as_a_string() {
        let id: UserId = "0190a5d0-0000-7000-8000-000000000001".parse().unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"0190a5d0-0000-7000-8000-000000000001\"", "serde form");
    }
}
