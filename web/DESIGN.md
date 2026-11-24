# Page design

Every page follows one of four archetypes. The primitives live in `src/components/ui`; ESLint catches the common breaks of the layout rules in `src/pages` (`grid-cols-*`, `col-span-*`, `columns-*`, `grid-flow-col` and `max-w-*`); the rest is on review.

## Surfaces and color

Colors live in `src/styles/theme.css` only. Surfaces step up like Grafana's, and a page picks its surfaces through the primitives, not by hand:

| Surface | Token | Use |
|---|---|---|
| Canvas | `base-200` | The page and the sidebar. |
| Panel | `base-100` | Cards, tables, stat tiles, description lists, side panels. |
| Raised | `raised` | Menus, popovers, tooltips and plain buttons. In light it is white, and the shadow gives the lift. |
| Field | `field`, `field-border` | Inputs, textareas and `Select` triggers: inset in dark, white with a strong border in light. The border meets 3:1. |
| Lines | `base-300` | Borders and dividers. |

`primary` is the orange accent: primary buttons, links (`link`), the active tab's underline, the active nav item's bar, focus rings and the first chart series. Keep muted text at `/70` or more so it passes AA on every surface.

## Building blocks

| Block | Use |
|---|---|
| `Page` | The root of every page. `width`: `wide` (default) fills the content column, for every page in the workspace shell; `compact` is a one-card page without a header, centered (Invite). Owns the space between top-level blocks. |
| `PageHeader` | A breadcrumb row, the title row (a 32px icon tile in `primary`, the title, actions on the right), the description (`max-w-prose`) and a `tabs` slot. The icon comes from the route's module nav item and the breadcrumb from the workspace shell (project or workspace, then the module, then the title unless the page is the module's own page), so pages pass neither; `icon` and `crumbs` override them for a page outside a module (Projects, Home). The last crumb is never a link, and a name still loading shows as a skeleton crumb, so pass `''` as the title while the entity loads rather than a placeholder word. Under `TabLayout` the breadcrumb stops at the module, since the tabs name the page. |
| `PageSection` | A titled block that isn't a card: a table or list with a heading and optional actions. |
| `Card` | A bordered group of fields or facts. `#actions` in the heading, `#footer` for a right-aligned Save. |
| `Field` | A label over its control, with a hint or error under it. It caps its own width so controls don't stretch on wide screens: `lg` (default, `max-w-3xl`) for text inputs, selects and textareas, `md` for short values such as names and IDs, `full` for a control that needs the room. |
| `Select`, `Menu` | Popups on the `raised` surface. Options and menu items are at least 36px tall with `px-3`; a `Select` list is at least 14rem wide whatever its trigger. `size`: `md` (default, 40px) in forms, `toolbar` (36px) in toolbars and section actions, `sm` (32px) inside table rows and inline editors. The list shifts to stay inside the viewport. |
| `DescriptionList` | Read-only facts about the entity, straight under the header. |
| `DetailActions` + `MenuItem` | A detail page's header actions: page buttons (`actions`), Save (`save`) and the "..." menu (`#menu`), all `btn-sm` in one style family. Save shows its own state: "Save", a disabled "Saved", or "Saving". Destructive menu items are `danger` and go last. |
| `FormPanel` | Create and edit forms, as a side panel or a dialog. |
| `Tabs` | Route links in a page header's `tabs` slot, one route per tab, for a feature with several pages (Analytics). A full-width line runs under the row and the active tab is underlined in `primary` with `aria-current="page"`. A tab stays active on paths under it unless `exact`. |
| `TabLayout`, `TabPage` | A feature with tabs is one parent route whose component wraps `TabLayout` (page, header and `Tabs` in the header, rendered once) with the tabs as child routes. Each tab's page renders a `TabPage`, which sets the header's title, description and actions, so a tab switch swaps only the content (Analytics, Mailing). Detail pages without tabs stay outside the parent. |
| `DateRangePicker` | A report's range: a preset `Select` (Today to Last month) and two date fields for Custom. Days are `YYYY-MM-DD` in UTC; `presetRange` and `previousPeriod` in `lib/range.ts` do the math. |
| `StatTiles` | A row of KPI tiles. With `previous` set, a tile shows the change as a percent with an arrow, green up and red down, or "new" when the previous value was 0. |
| `TimeSeriesChart` | A line chart over time in hand-written SVG. Each `primary` series takes the next chart color (`chart-1` to `chart-4`: primary, secondary, accent, success) and the first gets an area; a `muted` series is the dashed previous period. Hover or arrow keys read a point. Empty data draws a flat baseline. |
| `BarList` | A ranked list with a share bar behind each row (a `progress` element, so no width style), the top `limit` rows and a "Show all" button (`show-all`). |
| `ReportGrid` | Lays report panels out two-up on wide screens, one column below, so the page needs no grid classes. |

## List page

Projects, Team, API keys, Forms, Extensions.

- `Page` > `PageHeader` with the primary create button (`btn btn-primary btn-sm` with a Plus icon) > the main `DataTable` > more `PageSection`s.
- The main table's empty state carries the same create action. Other tables get an empty state without one.
- Row actions sit in the last column as icon buttons with an `aria-label`. Delete is a trash icon that confirms through `runAction`.

## Detail page

Form, Script. One column, always: no side-by-side cards or sidebars.

- `Page` (wide, so tables and editors fit) > `PageHeader` with the entity name as title; the breadcrumb (`Project / Module / Entity`) comes on its own.
- Header actions go through `DetailActions`: page-level buttons first, then Save, then the "..." menu with Delete last. Pages can't import `ActionsMenu` (ESLint), don't add their own buttons beside the group, and put no text label next to Save.
- Facts in a `DescriptionList` under the header. Editable groups in `Card`s, data in `PageSection`s.
- When the page edits the entity inline, one Save in the header (`DetailActions`' `save`) saves everything. Its label is the save state, so there is no separate status text.
- Config that isn't the page's main content opens in a `FormPanel` (form settings).

## Settings page

Workspace settings, analytics settings.

- `Page` (full width, like every page), one `Card` per group.
- Fields stack label over control. The card spans the page and `Field` caps each control (`width="md"` for short values such as names and IDs). Read-only values are readonly inputs with a `CopyButton`.
- Save sits in the card's footer and stays disabled until something changed. Each card saves on its own.
- A danger zone, when there is one, is the last card.

## Report page

Analytics. A read-only page of numbers for a date range.

- `Page` (wide) > `PageHeader` (with `Tabs` in its `tabs` slot when the feature has several pages) > a toolbar row with the `DateRangePicker` and the page's toggles (compare) > `StatTiles` > `TimeSeriesChart` > `ReportGrid` of `PageSection`s, each holding a `BarList`.
- The range lives in the URL query, so a link shares the exact view. Pass `today` to `DateRangePicker` from one place, in UTC.
- Each panel loads and fails on its own: a panel shows its own skeleton or error alert, and the others still render.
- A range with no data shows tiles at 0, a flat chart and "No data for this range" in each panel, not an empty-state card.
- "Show all" opens the full list in a side panel, not a new page.

## Rules for every page

- **Width.** Every top-level block on a page is full width, so blocks line up with each other and from page to page. Narrow content goes inside a block: `Field` caps its control, and `PageHeader`, `SectionHeading` (and so `Card`) cap their descriptions at `max-w-prose`. Side-by-side panels exist only inside `ReportGrid`.
- **Toolbars.** Every control in a toolbar row is 36px: `Select size="toolbar"`, `input h-9`, and a button beside them `btn h-9`. Controls in forms are 40px; controls inside table rows and inline editors are 32px (`Select size="sm"`, `input-sm`).
- **Spacing.** `Page` puts `gap-8` between blocks, `PageSection` `gap-3` under its heading, `Card` `gap-4` in its body. Pages add no margins between their top-level blocks.
- **Empty states.** `DataTable`'s `empty` prop: an icon, a title and one sentence. Only the main list adds a create action.
- **Forms.** `Field` puts the label over the control. One column. The submit button goes bottom right in panels, in the card footer on settings.
- **Destructive actions.** Always confirm with `runAction({ confirm })` and name the thing. Never a primary-colored destructive button outside the confirm dialog. On detail pages, Delete lives in the `DetailActions` menu.
- **Loading.** Skeletons on first load (`SkeletonRows`, `DataTable` handles its own), `SubmitButton`'s `pending` while saving.
- **Grids.** A page that needs a grid (the workspace cards on Home) puts it in a component, so the page itself stays one column.

## Command palette

- Cmd/Ctrl+K, or Search at the top of the sidebar, opens it anywhere in a workspace. Everything a user can reach from the sidebar or a list page should also be a palette row.
- A row's title is the thing itself ("Contact", "New form"). The palette draws `Forms ›` in front of it in search results, and uses the group as a section header on the empty view, so titles never repeat their container.
- Scopes filter the one list: All, Go to (`@`) for pages and objects, Actions (`>`) for things that do something. Tab cycles them.
- A "New ..." row opens the list page with its create panel (`?new=1`). Create forms stay on their pages, not in the palette.
