# NodeObject

Inherits: [BaseGameObject](basegameobject.md)

Inherited by: [SoundNode](soundnode.md), [FromBytes](frombytes.md), [ToSpeaker](tospeaker.md), [ToBytes](tobytes.md), [SoundModifier](soundmodifier.md)

The base type of every sound node.

## Description

A node makes sound, changes sound or takes sound out of luv. You make nodes with the [Sound API](sound-api.md) of a window. You connect them through their ports. See [NodeInput and NodeOutput](nodeports.md).

It also has every member of [BaseGameObject](basegameobject.md). `Name` starts out the same as `ClassName`. `tostring(node)` returns the `Name`.

## Node types

| Type | Made with | ClassName | Input | Output |
| --- | --- | --- | --- | --- |
| [SoundNode](soundnode.md) | [Sound:SoundNode](sound-api.md#soundnode) | `"SoundNode"` | yes | no |
| [FromString](soundnode.md#fromstring) | [Sound:FromString](sound-api.md#fromstring) | `"FromString"` | yes | no |
| [FromBytes](frombytes.md) | [Sound:FromBytes](sound-api.md#frombytes) | `"FromBytes"` | yes | no |
| [ToSpeaker](tospeaker.md) | [Sound:ToSpeaker](sound-api.md#tospeaker) | `"ToSpeaker"` | no | yes |
| [ToBytes](tobytes.md) | [Sound:ToBytes](sound-api.md#tobytes) | `"ToBytes"` | no | yes |
| [SoundModifier](soundmodifier.md) | [Sound:Modifier](sound-api.md#modifier) | the kind, like `"Gain"` | yes | yes |

Nodes that make sound only have an `Input`. Nodes that take sound out only have an `Output`. Modifiers have both. Reading a port that a node does not have errors, for example with `Output is not a valid member of SoundNode`.

## Lifetime

The window keeps every node alive. A node does not go away when your script stops using it. Call [Destroy](basegameobject.md#destroy) when you do not need a node anymore.

Destroy does this:

- The sound of the node fades out over 10 ms.
- Every link to and from the node is removed.
- The signals of the node are destroyed.

After Destroy, every member except `Name` and `ClassName` errors with `<ClassName> '<Name>' has been destroyed`.

When the window closes, luv destroys all of its nodes. [Sound:GetNodes](sound-api.md#getnodes) lists the nodes that are still alive.

## Config tables

Every function that makes a node takes an optional config table. luv sets each field on the new node, just like setting the property yourself. Keys must be strings. If one field fails, luv destroys the new node and raises the error. Each node page lists its config fields.

## Setting properties

- Numbers outside the range of a property are clamped. They do not error. Setting `Volume` to `50` gives `10`.
- Properties that only take whole numbers round the value.
- Boolean properties only take `true` or `false`.
- Enum properties only take an item of the right enum.
- Changes reach the audio when your script yields. So everything you change in one go takes effect at the same moment. That includes one frame of code or one [Bulk.BulkUpdate](bulk.md) call.
- Values from a config table apply at once. Later changes to a `Volume` glide over 20 ms, so they do not click.
- [Bulk.BulkUpdate](bulk.md) works with every property that you can set.

## Errors

| Message | Cause |
| --- | --- |
| `Loudness is not a valid member of SoundNode` | The node has no member with that name. |
| `IsPlaying is read-only on SoundNode` | The member is read only. |
| `Volume must be a number, got string` | The property takes a number. |
| `Volume must be a finite number` | The number is NaN or infinity. |
| `Looping must be a boolean, got number` | The property takes a boolean. |
| `expected an enum.RollOffMode item, got enum.AudioFormat.Int16` | The item is from another enum. |
| `config keys must be strings, got number` | A config table has a key that is not a string. |
| `SoundNode 'Music' has been destroyed` | The node was destroyed. |
| `this sound node belongs to a window that is closed` | The window of the node is closed. |
