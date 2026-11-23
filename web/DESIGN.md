# Page design

Every page follows one of three archetypes. The primitives live in `src/components/ui`; ESLint enforces the layout rules in `src/pages` (no `grid-cols-*`, `col-span-*` or `max-w-*` there).

## Building blocks

| Block | Use |
|---|---|
| `Page` | The root of every page. `width`: `wide` (default) for list and detail pages, `narrow` for settings, `compact` for a one-card page. Owns the space between top-level blocks. |
| `PageHeader` | Eyebrow (`section`), the title with actions on the same row, then the description. |
| `PageSection` | A titled block that isn't a card: a table or list with a heading and optional actions. |
| `Card` | A bordered group of fields or facts. `#actions` in the heading, `#footer` for a right-aligned Save. |
| `DescriptionList` | Read-only facts about the entity, straight under the header. |
| `ActionsMenu` + `MenuItem` | The "..." menu in a header for secondary actions. Destructive items are `danger` and go last. |
| `SaveStatus` | "Saved" or "Unsaved changes" next to an inline-edit Save. |
| `FormPanel` | Create and edit forms, as a side panel or a dialog. |

## List page

Projects, Team, API keys, Forms, Extensions.

- `Page` > `PageHeader` with the primary create button (`btn btn-primary btn-sm` with a Plus icon) > the main `DataTable` > more `PageSection`s.
- The main table's empty state carries the same create action. Other tables get an empty state without one.
- Row actions sit in the last column as icon buttons with an `aria-label`. Delete is a trash icon that confirms through `runAction`.

## Detail page

Form, Script. One column, always: no side-by-side cards or sidebars.

- `Page` (wide, so tables and editors fit) > `PageHeader` with the entity name as title and the project as eyebrow.
- Header actions: page-level buttons first, then `ActionsMenu` with Delete last.
- Facts in a `DescriptionList` under the header. Editable groups in `Card`s, data in `PageSection`s.
- When the page edits the entity inline, one Save in the header with `SaveStatus` saves everything, so it stays reachable above long editors.
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
- **Destructive actions.** Always confirm with `runAction({ confirm })` and name the thing. Never a primary-colored destructive button outside the confirm dialog. On detail pages, Delete lives in `ActionsMenu`.
- **Loading.** Skeletons on first load (`SkeletonRows`, `DataTable` handles its own), `SubmitButton`'s `pending` while saving.
- **Grids.** A page that needs a grid (the workspace cards on Home) puts it in a component, so the page itself stays one column.
