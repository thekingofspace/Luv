# 1.9

## Faster

- `Bulk.BulkUpdate` does the whole batch in one step instead of one step per property. Setting two properties on 900 renderables went from 9.3 ms to 4.1 ms.

## Assets

- `Asset.FromBytes` makes an asset out of bytes you already have, with no file on disk.
- `Asset.FromBase64` does the same from base64 text, and takes a data URL as it comes.
- Plugins can hand Luau an asset with `push_asset`, so C can build a picture and pass it straight to a RenderableImage.

## Mods

- `ECall` reads a Luau file from outside the game, compiles it and hands back an `ExternalModule`.
- `Fetch` runs it once and keeps what it returned. `Drop` forgets that, so the next `Fetch` runs it again.
- Nothing is copied into the game and nothing is written to disk.

## Plugins

- Your editor leaves plugin type files alone, so a file that only makes sense once luv folds it in does not get marked.
