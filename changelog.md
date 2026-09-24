# 1.8

## Faster

- `Bulk.BulkUpdate` does the whole batch in one step instead of one step per property. Setting two properties on 900 renderables went from 9.3 ms to 4.1 ms.

## Plugins

- Your editor leaves plugin type files alone, so a file that only makes sense once luv folds it in does not get marked.
