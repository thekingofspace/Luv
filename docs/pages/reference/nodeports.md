# NodeInput and NodeOutput

The ports of a sound node. Links between ports decide where sound goes.

## Description

You get ports from the `Input` and `Output` properties of a node. Reading a port again gives the same object, so you can compare ports with `==`. [NodeObject](nodeobject.md#node-types) lists which nodes have which port.

`typeof(port)` is `"NodeInput"` or `"NodeOutput"`. `tostring(port)` gives the node name and the port, like `"Music.Input"`.

## Direction

Sound flows from the node that owns the Input to the node that owns the Output. `a.Input:Link(b.Output)` sends the sound of `a` into `b`.

- A SoundNode, FromString or FromBytes only has an Input. Its sound leaves through it.
- A ToSpeaker or ToBytes only has an Output. Sound arrives through it.
- A modifier has both. Sound arrives at its Output and leaves through its Input.

This tree shows a typical chain. Each node sends its sound to the node above it.

```tree
speaker (ToSpeaker)
└── reverb (Reverb)
    ├── music (SoundNode)
    └── click (FromString)
```

| Node | The link that puts it there |
| --- | --- |
| `reverb` | `reverb.Input:Link(speaker.Output)` |
| `music` | `music.Input:Link(reverb.Output)` |
| `click` | `click.Input:Link(reverb.Output)` |

You can make each link from the other side too. `speaker.Output:Link(reverb.Input)` makes the same link as `reverb.Input:Link(speaker.Output)`.

## Layering

- An Output can take many links. luv adds their sound together at full volume. It does not average them. Two sounds at `0.25` give `0.5`.
- An Input can link to many Outputs. Each one gets the same sound. Use this to hear a sound and record it with a [ToBytes](tobytes.md) node at the same time.
- Links cannot form a loop.
- Links fade in and out over 5 ms, so linking does not click.
- A node with no links keeps running. A playing SoundNode still moves forward and fires its signals. You just do not hear it.

## Linking a node

Every method that takes a port also takes a node. luv uses the matching port of that node. `music.Input:Link(speaker)` links to `speaker.Output`.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `Node` | [NodeObject](nodeobject.md) | The node that owns this port. Read only. |

Reading `Node` errors with `this sound node has been destroyed` after the node is destroyed.

## Methods

### Link

```luau
input:Link(target: NodeOutput | NodeObject): boolean
output:Link(source: NodeInput | NodeObject): boolean
```

Links two ports. Returns `true` when it made a new link. Returns `false` when the link was already there.

It errors when:

| Message | Cause |
| --- | --- |
| `an Input can only link to an Output or to a node that has one` | You passed an Input, or a value that is not a port or a node. |
| `an Output can only link to an Input or to a node that has one` | You passed an Output, or a value that is not a port or a node. |
| `SoundNode has no Output to link to` | The node you passed does not have the port that is needed. |
| `a sound node cannot link to itself` | Both ports belong to the same node. |
| `linking these nodes would make the sound loop back into itself` | The link would make a loop. |
| `sound nodes from different windows cannot be linked` | The nodes belong to two windows. |
| `the node to link to has been destroyed` | The other port belongs to a destroyed node. |
| `this sound node has been destroyed` | This port belongs to a destroyed node. |

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local music = Sound:SoundNode("music/theme.ogg")
local echo = Sound:Modifier("Echo")
local speaker = Sound:ToSpeaker()
print(music.Input:Link(echo.Output))
print(echo.Output:Link(music.Input))
echo.Input:Link(speaker)
```

The first `print` shows `true`. The second shows `false`, because it is the same link.

### Unlink

```luau
input:Unlink(target: (NodeOutput | NodeObject)?): boolean
output:Unlink(source: (NodeInput | NodeObject)?): boolean
```

Removes the link to the given port or node. With no argument, it removes every link of this port. Returns `true` if it removed a link. The argument is checked the same way as in [Link](#link).

### IsLinked

```luau
input:IsLinked(target: (NodeOutput | NodeObject)?): boolean
output:IsLinked(source: (NodeInput | NodeObject)?): boolean
```

Returns `true` if this port is linked to the given port or node. With no argument, returns `true` if this port has any link.

### GetLinks

```luau
input:GetLinks(): { NodeOutput }
output:GetLinks(): { NodeInput }
```

An Input returns the Outputs it sends sound to. An Output returns the Inputs that send sound into it. Use `Node` to get from a port to its node.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local music = Sound:SoundNode("music/theme.ogg", { Name = "Music" })
local speaker = Sound:ToSpeaker()
music.Input:Link(speaker)
for _, input in speaker.Output:GetLinks() do
	print(input.Node.Name)
end
```
