# Page design

Every page follows one of three archetypes. The primitives live in `src/components/ui`; ESLint catches the common breaks of the layout rules in `src/pages` (`grid-cols-*`, `col-span-*`, `columns-*`, `grid-flow-col` and `max-w-*`); the rest is on review.

## Building blocks

| Block | Use |
|---|---|
| `Page` | The root of every page. `width`: `wide` (default) for list and detail pages, `narrow` for settings, `compact` for a one-card page without a header, centered. Owns the space between top-level blocks. |
| `PageHeader` | Eyebrow (`section`), the title with actions on the same row, then the description. |
| `PageSection` | A titled block that isn't a card: a table or list with a heading and optional actions. |
| `Card` | A bordered group of fields or facts. `#actions` in the heading, `#footer` for a right-aligned Save. |
| `DescriptionList` | Read-only facts about the entity, straight under the header. |
| `DetailActions` + `MenuItem` | A detail page's header actions: page buttons (`actions`), Save (`save`) and the "..." menu (`#menu`), all `btn-sm` in one style family. Save shows its own state: "Save", a disabled "Saved", or "Saving". Destructive menu items are `danger` and go last. |
| `FormPanel` | Create and edit forms, as a side panel or a dialog. |

## List page

Projects, Team, API keys, Forms, Extensions.

- `Page` > `PageHeader` with the primary create button (`btn btn-primary btn-sm` with a Plus icon) > the main `DataTable` > more `PageSection`s.
- The main table's empty state carries the same create action. Other tables get an empty state without one.
- Row actions sit in the last column as icon buttons with an `aria-label`. Delete is a trash icon that confirms through `runAction`.

## Detail page

Form, Script. One column, always: no side-by-side cards or sidebars.

- `Page` (wide, so tables and editors fit) > `PageHeader` with the entity name as title and the project as eyebrow.
- Header actions go through `DetailActions`: page-level buttons first, then Save, then the "..." menu with Delete last. Pages can't import `ActionsMenu` or style header buttons themselves, and no text label sits next to Save.
- Facts in a `DescriptionList` under the header. Editable groups in `Card`s, data in `PageSection`s.
- When the page edits the entity inline, one Save in the header (`DetailActions`' `save`) saves everything. Its label is the save state, so there is no separate status text.
- Config that isn't the page's main content opens in a `FormPanel` (form settings).

## Settings page

Workspace settings.

- `Page width="narrow"`, one `Card` per group.
- Fields stack label over control at full width. Read-only values are readonly inputs with a `CopyButton`.
- Save sits in the card's footer and stays disabled until something changed. Each card saves on its own.
- A danger zone, when there is one, is the last card.

## Rules for every page

- **Spacing.** `Page` puts `gap-8` between blocks, `PageSection` `gap-3` under its heading, `Card` `gap-4` in its body. Pages add no margins between their top-level blocks.
- **Empty states.** `DataTable`'s `empty` prop: an icon, a title and one sentence. Only the main list adds a create action.
- **Forms.** `Field` puts the label over the control. One column. The submit button goes bottom right in panels, in the card footer on settings.
- **Destructive actions.** Always confirm with `runAction({ confirm })` and name the thing. Never a primary-colored destructive button outside the confirm dialog. On detail pages, Delete lives in the `DetailActions` menu.
- **Loading.** Skeletons on first load (`SkeletonRows`, `DataTable` handles its own), `SubmitButton`'s `pending` while saving.
- **Grids.** A page that needs a grid (the workspace cards on Home) puts it in a component, so the page itself stays one column.
