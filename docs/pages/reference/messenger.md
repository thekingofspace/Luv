# Messenger

Inherits: [BaseGameObject](basegameobject.md)

Sends messages by topic between the main thread and parallel blocks.

```luau
local Messenger = import("Messenger")
```

## Description

`import("Messenger")` returns the Messenger object of the current thread. Each parallel block has its own. Call its methods with `:`.

A message has a topic and any number of values. [Fire](#fire) sends it. [Subscribe](#subscribe) and [Wait](#wait) receive it. A topic is any string. Topics must match exactly, and case matters.

A message goes to every thread. That means the main thread and every parallel block that is still running. The thread that fires it gets it too.

It also has every member of [BaseGameObject](basegameobject.md). Its `ClassName` and `Name` are both `"Messenger"`.

```luau
local Messenger = import("Messenger")

Messenger:Subscribe("Chat", function(sender: string, text: string)
	print(`{sender}: {text}`)
end)
Messenger:Fire("Chat", "ada", "hello")
print("sent")
```

This prints `sent` and then `ada: hello`.

## Delivery rules

- Fire never runs handlers right away, not even in the same thread. They run after the code that fired yields or ends.
- Each thread handles its messages one at a time, in the order they arrive.
- When a message arrives, each subscription for its topic starts on its own coroutine, in the order you subscribed. Coroutines that wait for the topic resume right after.
- A message only reaches the subscriptions and waits that exist when it arrives. luv does not keep it for later.
- A parallel block gets every message sent after the block starts. Its own code runs before its first message, so subscribe at the top of the block.
- An error in a subscription handler prints as an uncaught error.
- Subscriptions and waits do not keep the game running. A message that has not reached every thread yet does.

## Values you can send

luv copies the values when you call Fire. Later changes to a table do not change the message.

| Value | What arrives |
| --- | --- |
| `nil`, booleans, numbers and strings | The same value. |
| `vector` | The same value. |
| `buffer` | A copy of the buffer. |
| Tables | A deep copy. Keys and values follow the rules in this table. Metatables are not copied. |
| [UDim](udim.md) and [Color](color.md) | A copy. |
| Enum items | The same item. |
| Anything from `import`, including Messenger | The receiving thread's own copy. |
| Functions and coroutines | Cannot be sent. |
| Engine objects, like a [Signal](signal.md) or a [Window](window.md) | Cannot be sent. |
| Tables that hold a value that cannot be sent | Cannot be sent. |
| Tables that contain themselves | Cannot be sent. |
| Tables nested deeper than 128 levels | Cannot be sent. |

A table can hold the same inner table twice. Each spot gets its own copy.

A value that cannot be sent raises an error like `cannot fire 'Hit': function values cannot be sent between threads`.

## Methods

### Subscribe

```luau
Messenger:Subscribe(topic: string, handler: (...any) -> ()): number
```

Runs `handler` with the values of every message on this topic. Returns an id for [Unsubscribe](#unsubscribe). In each thread the ids start at 1 and go up by one. A topic can have many subscriptions.

### Unsubscribe

```luau
Messenger:Unsubscribe(id: number): boolean
```

Removes a subscription. Returns `true` when it was removed and `false` when the id is unknown.

```luau
local Messenger = import("Messenger")

local chat = Messenger:Subscribe("Chat", function(text: string)
	print(text)
end)
Messenger:Subscribe("Mute", function()
	Messenger:Unsubscribe(chat)
end)

Messenger:Fire("Chat", "hello")
Messenger:Fire("Mute")
Messenger:Fire("Chat", "anyone there?")
```

This prints only `hello`. The last message arrives after the Mute handler removed the subscription.

### Fire

```luau
Messenger:Fire(topic: string, ...: any)
```

Sends a message to every thread. It copies the values right away and never yields. See [Values you can send](#values-you-can-send).

### Wait

```luau
Messenger:Wait(topic: string): ...any
```

Yields the calling coroutine until the next message on this topic reaches this thread. Returns the values of that message. Every coroutine that waits on the topic gets the same message.

When the Messenger is destroyed during the wait, Wait raises `Messenger was destroyed while waiting for '<topic>'`.

A waiting coroutine does not keep the game running.

```luau
local Messenger = import("Messenger")

coroutine.wrap(function()
	local level, spawns = Messenger:Wait("LevelLoaded")
	print(level, #spawns)
end)()

local points = { udim.new(10, 20), udim.new(30, 40) }
Messenger:Fire("LevelLoaded", "Caves", points)
table.clear(points)
```

This prints `Caves` and `2`. The message holds a copy, so clearing the table later changes nothing.

### Destroy

```luau
Messenger:Destroy()
```

Removes every subscription. Each waiting coroutine gets the error from [Wait](#wait). After this, Subscribe, Fire and Wait raise `Messenger 'Messenger' has been destroyed`. Unsubscribe returns `false`. See [BaseGameObject](basegameobject.md#destroy).

> [!WARNING]
> `import("Messenger")` always returns the same object. Once you destroy it, Messenger stops working in that thread for the rest of the game.
