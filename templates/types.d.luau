export type EnumItem = {
	Name: string,
	Value: number,
	EnumType: string,
}

export type WindowTypeEnum = {
	Name: "Windowed" | "Borderless" | "Maximized" | "FullScreen" | "ExclusiveFullScreen",
	Value: number,
	EnumType: "WindowType",
}

export type WindowType_Enum = {
	Windowed: WindowTypeEnum,
	Borderless: WindowTypeEnum,
	Maximized: WindowTypeEnum,
	FullScreen: WindowTypeEnum,
	ExclusiveFullScreen: WindowTypeEnum,
	GetEnumItems: (self: WindowType_Enum) -> { WindowTypeEnum },
}

export type ShapeTypeEnum = {
	Name: "Rectangle" | "Circle" | "Triangle" | "RightTriangle" | "Diamond" | "Pentagon" | "Hexagon" | "Octagon",
	Value: number,
	EnumType: "ShapeType",
}

export type ShapeType_Enum = {
	Rectangle: ShapeTypeEnum,
	Circle: ShapeTypeEnum,
	Triangle: ShapeTypeEnum,
	RightTriangle: ShapeTypeEnum,
	Diamond: ShapeTypeEnum,
	Pentagon: ShapeTypeEnum,
	Hexagon: ShapeTypeEnum,
	Octagon: ShapeTypeEnum,
	GetEnumItems: (self: ShapeType_Enum) -> { ShapeTypeEnum },
}

export type TextXAlignmentEnum = {
	Name: "Left" | "Center" | "Right",
	Value: number,
	EnumType: "TextXAlignment",
}

export type TextXAlignment_Enum = {
	Left: TextXAlignmentEnum,
	Center: TextXAlignmentEnum,
	Right: TextXAlignmentEnum,
	GetEnumItems: (self: TextXAlignment_Enum) -> { TextXAlignmentEnum },
}

export type TextYAlignmentEnum = {
	Name: "Top" | "Center" | "Bottom",
	Value: number,
	EnumType: "TextYAlignment",
}

export type TextYAlignment_Enum = {
	Top: TextYAlignmentEnum,
	Center: TextYAlignmentEnum,
	Bottom: TextYAlignmentEnum,
	GetEnumItems: (self: TextYAlignment_Enum) -> { TextYAlignmentEnum },
}

export type ResampleModeEnum = {
	Name: "Smooth" | "Pixelated",
	Value: number,
	EnumType: "ResampleMode",
}

export type ResampleMode_Enum = {
	Smooth: ResampleModeEnum,
	Pixelated: ResampleModeEnum,
	GetEnumItems: (self: ResampleMode_Enum) -> { ResampleModeEnum },
}

export type BlendModeEnum = {
	Name: "Alpha" | "Additive" | "Multiply" | "Opaque",
	Value: number,
	EnumType: "BlendMode",
}

export type BlendMode_Enum = {
	Alpha: BlendModeEnum,
	Additive: BlendModeEnum,
	Multiply: BlendModeEnum,
	Opaque: BlendModeEnum,
	GetEnumItems: (self: BlendMode_Enum) -> { BlendModeEnum },
}

export type HashAlgorithmEnum = {
	Name: "MD5" | "SHA1" | "SHA224" | "SHA256" | "SHA384" | "SHA512" | "SHA3_256" | "SHA3_384" | "SHA3_512" | "BLAKE3",
	Value: number,
	EnumType: "HashAlgorithm",
}

export type HashAlgorithm_Enum = {
	MD5: HashAlgorithmEnum,
	SHA1: HashAlgorithmEnum,
	SHA224: HashAlgorithmEnum,
	SHA256: HashAlgorithmEnum,
	SHA384: HashAlgorithmEnum,
	SHA512: HashAlgorithmEnum,
	SHA3_256: HashAlgorithmEnum,
	SHA3_384: HashAlgorithmEnum,
	SHA3_512: HashAlgorithmEnum,
	BLAKE3: HashAlgorithmEnum,
	GetEnumItems: (self: HashAlgorithm_Enum) -> { HashAlgorithmEnum },
}

export type CipherAlgorithmEnum = {
	Name: "AES128GCM" | "AES256GCM" | "ChaCha20Poly1305",
	Value: number,
	EnumType: "CipherAlgorithm",
}

export type CipherAlgorithm_Enum = {
	AES128GCM: CipherAlgorithmEnum,
	AES256GCM: CipherAlgorithmEnum,
	ChaCha20Poly1305: CipherAlgorithmEnum,
	GetEnumItems: (self: CipherAlgorithm_Enum) -> { CipherAlgorithmEnum },
}

export type KeyAlgorithmEnum = {
	Name: "Ed25519" | "EcdsaP256" | "EcdsaP384" | "X25519",
	Value: number,
	EnumType: "KeyAlgorithm",
}

export type KeyAlgorithm_Enum = {
	Ed25519: KeyAlgorithmEnum,
	EcdsaP256: KeyAlgorithmEnum,
	EcdsaP384: KeyAlgorithmEnum,
	X25519: KeyAlgorithmEnum,
	GetEnumItems: (self: KeyAlgorithm_Enum) -> { KeyAlgorithmEnum },
}

export type KeyCodeEnum = {
	Name: "Unknown"
		| "A"
		| "B"
		| "C"
		| "D"
		| "E"
		| "F"
		| "G"
		| "H"
		| "I"
		| "J"
		| "K"
		| "L"
		| "M"
		| "N"
		| "O"
		| "P"
		| "Q"
		| "R"
		| "S"
		| "T"
		| "U"
		| "V"
		| "W"
		| "X"
		| "Y"
		| "Z"
		| "Zero"
		| "One"
		| "Two"
		| "Three"
		| "Four"
		| "Five"
		| "Six"
		| "Seven"
		| "Eight"
		| "Nine"
		| "F1"
		| "F2"
		| "F3"
		| "F4"
		| "F5"
		| "F6"
		| "F7"
		| "F8"
		| "F9"
		| "F10"
		| "F11"
		| "F12"
		| "F13"
		| "F14"
		| "F15"
		| "F16"
		| "F17"
		| "F18"
		| "F19"
		| "F20"
		| "F21"
		| "F22"
		| "F23"
		| "F24"
		| "Space"
		| "Return"
		| "Escape"
		| "Tab"
		| "Backspace"
		| "Delete"
		| "Insert"
		| "Home"
		| "End"
		| "PageUp"
		| "PageDown"
		| "Up"
		| "Down"
		| "Left"
		| "Right"
		| "LeftShift"
		| "RightShift"
		| "LeftControl"
		| "RightControl"
		| "LeftAlt"
		| "RightAlt"
		| "LeftSuper"
		| "RightSuper"
		| "CapsLock"
		| "NumLock"
		| "ScrollLock"
		| "PrintScreen"
		| "Pause"
		| "Menu"
		| "Backquote"
		| "Minus"
		| "Equals"
		| "LeftBracket"
		| "RightBracket"
		| "Backslash"
		| "Semicolon"
		| "Quote"
		| "Comma"
		| "Period"
		| "Slash"
		| "KeypadZero"
		| "KeypadOne"
		| "KeypadTwo"
		| "KeypadThree"
		| "KeypadFour"
		| "KeypadFive"
		| "KeypadSix"
		| "KeypadSeven"
		| "KeypadEight"
		| "KeypadNine"
		| "KeypadPeriod"
		| "KeypadDivide"
		| "KeypadMultiply"
		| "KeypadMinus"
		| "KeypadPlus"
		| "KeypadEnter"
		| "KeypadEquals",
	Value: number,
	EnumType: "KeyCode",
}

export type KeyCode_Enum = {
	Unknown: KeyCodeEnum,
	A: KeyCodeEnum,
	B: KeyCodeEnum,
	C: KeyCodeEnum,
	D: KeyCodeEnum,
	E: KeyCodeEnum,
	F: KeyCodeEnum,
	G: KeyCodeEnum,
	H: KeyCodeEnum,
	I: KeyCodeEnum,
	J: KeyCodeEnum,
	K: KeyCodeEnum,
	L: KeyCodeEnum,
	M: KeyCodeEnum,
	N: KeyCodeEnum,
	O: KeyCodeEnum,
	P: KeyCodeEnum,
	Q: KeyCodeEnum,
	R: KeyCodeEnum,
	S: KeyCodeEnum,
	T: KeyCodeEnum,
	U: KeyCodeEnum,
	V: KeyCodeEnum,
	W: KeyCodeEnum,
	X: KeyCodeEnum,
	Y: KeyCodeEnum,
	Z: KeyCodeEnum,
	Zero: KeyCodeEnum,
	One: KeyCodeEnum,
	Two: KeyCodeEnum,
	Three: KeyCodeEnum,
	Four: KeyCodeEnum,
	Five: KeyCodeEnum,
	Six: KeyCodeEnum,
	Seven: KeyCodeEnum,
	Eight: KeyCodeEnum,
	Nine: KeyCodeEnum,
	F1: KeyCodeEnum,
	F2: KeyCodeEnum,
	F3: KeyCodeEnum,
	F4: KeyCodeEnum,
	F5: KeyCodeEnum,
	F6: KeyCodeEnum,
	F7: KeyCodeEnum,
	F8: KeyCodeEnum,
	F9: KeyCodeEnum,
	F10: KeyCodeEnum,
	F11: KeyCodeEnum,
	F12: KeyCodeEnum,
	F13: KeyCodeEnum,
	F14: KeyCodeEnum,
	F15: KeyCodeEnum,
	F16: KeyCodeEnum,
	F17: KeyCodeEnum,
	F18: KeyCodeEnum,
	F19: KeyCodeEnum,
	F20: KeyCodeEnum,
	F21: KeyCodeEnum,
	F22: KeyCodeEnum,
	F23: KeyCodeEnum,
	F24: KeyCodeEnum,
	Space: KeyCodeEnum,
	Return: KeyCodeEnum,
	Escape: KeyCodeEnum,
	Tab: KeyCodeEnum,
	Backspace: KeyCodeEnum,
	Delete: KeyCodeEnum,
	Insert: KeyCodeEnum,
	Home: KeyCodeEnum,
	End: KeyCodeEnum,
	PageUp: KeyCodeEnum,
	PageDown: KeyCodeEnum,
	Up: KeyCodeEnum,
	Down: KeyCodeEnum,
	Left: KeyCodeEnum,
	Right: KeyCodeEnum,
	LeftShift: KeyCodeEnum,
	RightShift: KeyCodeEnum,
	LeftControl: KeyCodeEnum,
	RightControl: KeyCodeEnum,
	LeftAlt: KeyCodeEnum,
	RightAlt: KeyCodeEnum,
	LeftSuper: KeyCodeEnum,
	RightSuper: KeyCodeEnum,
	CapsLock: KeyCodeEnum,
	NumLock: KeyCodeEnum,
	ScrollLock: KeyCodeEnum,
	PrintScreen: KeyCodeEnum,
	Pause: KeyCodeEnum,
	Menu: KeyCodeEnum,
	Backquote: KeyCodeEnum,
	Minus: KeyCodeEnum,
	Equals: KeyCodeEnum,
	LeftBracket: KeyCodeEnum,
	RightBracket: KeyCodeEnum,
	Backslash: KeyCodeEnum,
	Semicolon: KeyCodeEnum,
	Quote: KeyCodeEnum,
	Comma: KeyCodeEnum,
	Period: KeyCodeEnum,
	Slash: KeyCodeEnum,
	KeypadZero: KeyCodeEnum,
	KeypadOne: KeyCodeEnum,
	KeypadTwo: KeyCodeEnum,
	KeypadThree: KeyCodeEnum,
	KeypadFour: KeyCodeEnum,
	KeypadFive: KeyCodeEnum,
	KeypadSix: KeyCodeEnum,
	KeypadSeven: KeyCodeEnum,
	KeypadEight: KeyCodeEnum,
	KeypadNine: KeyCodeEnum,
	KeypadPeriod: KeyCodeEnum,
	KeypadDivide: KeyCodeEnum,
	KeypadMultiply: KeyCodeEnum,
	KeypadMinus: KeyCodeEnum,
	KeypadPlus: KeyCodeEnum,
	KeypadEnter: KeyCodeEnum,
	KeypadEquals: KeyCodeEnum,
	GetEnumItems: (self: KeyCode_Enum) -> { KeyCodeEnum },
}

export type MouseButtonEnum = {
	Name: "Left" | "Right" | "Middle" | "Back" | "Forward",
	Value: number,
	EnumType: "MouseButton",
}

export type MouseButton_Enum = {
	Left: MouseButtonEnum,
	Right: MouseButtonEnum,
	Middle: MouseButtonEnum,
	Back: MouseButtonEnum,
	Forward: MouseButtonEnum,
	GetEnumItems: (self: MouseButton_Enum) -> { MouseButtonEnum },
}

export type MouseIconEnum = {
	Name: "Default"
		| "Pointer"
		| "Text"
		| "Crosshair"
		| "Wait"
		| "Progress"
		| "Move"
		| "NotAllowed"
		| "Grab"
		| "Grabbing"
		| "Help"
		| "ResizeHorizontal"
		| "ResizeVertical"
		| "ResizeDiagonalDown"
		| "ResizeDiagonalUp"
		| "ZoomIn"
		| "ZoomOut"
		| "Cell"
		| "Copy"
		| "ContextMenu",
	Value: number,
	EnumType: "MouseIcon",
}

export type MouseIcon_Enum = {
	Default: MouseIconEnum,
	Pointer: MouseIconEnum,
	Text: MouseIconEnum,
	Crosshair: MouseIconEnum,
	Wait: MouseIconEnum,
	Progress: MouseIconEnum,
	Move: MouseIconEnum,
	NotAllowed: MouseIconEnum,
	Grab: MouseIconEnum,
	Grabbing: MouseIconEnum,
	Help: MouseIconEnum,
	ResizeHorizontal: MouseIconEnum,
	ResizeVertical: MouseIconEnum,
	ResizeDiagonalDown: MouseIconEnum,
	ResizeDiagonalUp: MouseIconEnum,
	ZoomIn: MouseIconEnum,
	ZoomOut: MouseIconEnum,
	Cell: MouseIconEnum,
	Copy: MouseIconEnum,
	ContextMenu: MouseIconEnum,
	GetEnumItems: (self: MouseIcon_Enum) -> { MouseIconEnum },
}

export type MouseLockModeEnum = {
	Name: "None" | "Confined" | "Locked",
	Value: number,
	EnumType: "MouseLockMode",
}

export type MouseLockMode_Enum = {
	None: MouseLockModeEnum,
	Confined: MouseLockModeEnum,
	Locked: MouseLockModeEnum,
	GetEnumItems: (self: MouseLockMode_Enum) -> { MouseLockModeEnum },
}

export type ControllerButtonEnum = {
	Name: "A"
		| "B"
		| "X"
		| "Y"
		| "LeftBumper"
		| "RightBumper"
		| "LeftTrigger"
		| "RightTrigger"
		| "Select"
		| "Start"
		| "Home"
		| "LeftStick"
		| "RightStick"
		| "DPadUp"
		| "DPadDown"
		| "DPadLeft"
		| "DPadRight",
	Value: number,
	EnumType: "ControllerButton",
}

export type ControllerButton_Enum = {
	A: ControllerButtonEnum,
	B: ControllerButtonEnum,
	X: ControllerButtonEnum,
	Y: ControllerButtonEnum,
	LeftBumper: ControllerButtonEnum,
	RightBumper: ControllerButtonEnum,
	LeftTrigger: ControllerButtonEnum,
	RightTrigger: ControllerButtonEnum,
	Select: ControllerButtonEnum,
	Start: ControllerButtonEnum,
	Home: ControllerButtonEnum,
	LeftStick: ControllerButtonEnum,
	RightStick: ControllerButtonEnum,
	DPadUp: ControllerButtonEnum,
	DPadDown: ControllerButtonEnum,
	DPadLeft: ControllerButtonEnum,
	DPadRight: ControllerButtonEnum,
	GetEnumItems: (self: ControllerButton_Enum) -> { ControllerButtonEnum },
}

export type ControllerAxisEnum = {
	Name: "LeftStickX" | "LeftStickY" | "RightStickX" | "RightStickY" | "LeftTrigger" | "RightTrigger",
	Value: number,
	EnumType: "ControllerAxis",
}

export type ControllerAxis_Enum = {
	LeftStickX: ControllerAxisEnum,
	LeftStickY: ControllerAxisEnum,
	RightStickX: ControllerAxisEnum,
	RightStickY: ControllerAxisEnum,
	LeftTrigger: ControllerAxisEnum,
	RightTrigger: ControllerAxisEnum,
	GetEnumItems: (self: ControllerAxis_Enum) -> { ControllerAxisEnum },
}

export type ControllerStickEnum = {
	Name: "Left" | "Right",
	Value: number,
	EnumType: "ControllerStick",
}

export type ControllerStick_Enum = {
	Left: ControllerStickEnum,
	Right: ControllerStickEnum,
	GetEnumItems: (self: ControllerStick_Enum) -> { ControllerStickEnum },
}

export type AudioFormatEnum = {
	Name: "Float32" | "Int16",
	Value: number,
	EnumType: "AudioFormat",
}

export type AudioFormat_Enum = {
	Float32: AudioFormatEnum,
	Int16: AudioFormatEnum,
	GetEnumItems: (self: AudioFormat_Enum) -> { AudioFormatEnum },
}

export type RollOffModeEnum = {
	Name: "Inverse" | "Linear" | "LinearSquare" | "InverseTapered",
	Value: number,
	EnumType: "RollOffMode",
}

export type RollOffMode_Enum = {
	Inverse: RollOffModeEnum,
	Linear: RollOffModeEnum,
	LinearSquare: RollOffModeEnum,
	InverseTapered: RollOffModeEnum,
	GetEnumItems: (self: RollOffMode_Enum) -> { RollOffModeEnum },
}

export type Enums = {
	AudioFormat: AudioFormat_Enum,
	BlendMode: BlendMode_Enum,
	CipherAlgorithm: CipherAlgorithm_Enum,
	ControllerAxis: ControllerAxis_Enum,
	ControllerButton: ControllerButton_Enum,
	ControllerStick: ControllerStick_Enum,
	HashAlgorithm: HashAlgorithm_Enum,
	KeyAlgorithm: KeyAlgorithm_Enum,
	KeyCode: KeyCode_Enum,
	MouseButton: MouseButton_Enum,
	MouseIcon: MouseIcon_Enum,
	MouseLockMode: MouseLockMode_Enum,
	ResampleMode: ResampleMode_Enum,
	RollOffMode: RollOffMode_Enum,
	ShapeType: ShapeType_Enum,
	TextXAlignment: TextXAlignment_Enum,
	TextYAlignment: TextYAlignment_Enum,
	WindowType: WindowType_Enum,
}

export type UDim = setmetatable<{
	X: number,
	Y: number,
	Z: number,
	Lerp: (self: UDim, goal: UDim, alpha: number) -> UDim,
}, UDimMetatable>

export type UDimMetatable = {
	__add: (UDim, UDim) -> UDim,
	__sub: (UDim, UDim) -> UDim,
	__mul: (UDim | number, UDim | number) -> UDim,
	__div: (UDim | number, UDim | number) -> UDim,
	__unm: (UDim) -> UDim,
	__eq: (UDim, UDim) -> boolean,
	__tostring: (UDim) -> string,
}

export type UDim_API = {
	new: (x: number?, y: number?, z: number?) -> UDim,
	zero: UDim,
}

export type Color = setmetatable<{
	R: number,
	G: number,
	B: number,
	A: number,
	Lerp: (self: Color, goal: Color, alpha: number) -> Color,
	ToHex: (self: Color, includeAlpha: boolean?) -> string,
	ToRGB: (self: Color) -> (number, number, number, number),
	ToHSV: (self: Color) -> (number, number, number),
}, ColorMetatable>

export type ColorMetatable = {
	__add: (Color, Color) -> Color,
	__sub: (Color, Color) -> Color,
	__mul: (Color | number, Color | number) -> Color,
	__div: (Color | number, Color | number) -> Color,
	__eq: (Color, Color) -> boolean,
	__tostring: (Color) -> string,
}

export type Color_API = {
	new: (r: number?, g: number?, b: number?, a: number?) -> Color,
	fromRGB: (r: number, g: number, b: number, a: number?) -> Color,
	fromHex: (hex: string) -> Color,
	fromHSV: (h: number, s: number, v: number, a: number?) -> Color,
	white: Color,
	black: Color,
	transparent: Color,
}

export type Bulk_API = {
	BulkUpdate: (updates: { [any]: { [string]: any } }) -> (),
	BulkWriteShaderData: (shader: Shader, updates: { [Renderable]: { [string]: ShaderValue } }) -> (),
}

export type BaseGameObject = {
	ClassName: string,
	Name: string,
	Destroy: (self: BaseGameObject) -> (),
}

export type Signal<T... = ...any> = BaseGameObject & {
	BindHandler: (self: Signal<T...>, id: string, handler: (T...) -> ...any) -> (),
	UnBind: (self: Signal<T...>, id: string) -> boolean,
	IsBound: (self: Signal<T...>, id: string) -> boolean,
	Invoke: (self: Signal<T...>, id: string, T...) -> ...any,
	Fire: (self: Signal<T...>, T...) -> (),
	Wait: (self: Signal<T...>) -> T...,
}

export type Messenger = BaseGameObject & {
	Subscribe: (self: Messenger, topic: string, handler: (...any) -> ()) -> number,
	Unsubscribe: (self: Messenger, id: number) -> boolean,
	Fire: (self: Messenger, topic: string, ...any) -> (),
	Wait: (self: Messenger, topic: string) -> ...any,
}

export type ReadFormat = "n" | "a" | "l" | "L" | "*n" | "*a" | "*l" | "*L" | number

export type File = BaseGameObject & {
	read: (self: File, ...ReadFormat) -> ...any,
	write: (self: File, ...(string | number)) -> (File?, string?, number?),
	lines: (self: File, ...ReadFormat) -> () -> ...any,
	seek: (self: File, whence: ("set" | "cur" | "end")?, offset: number?) -> (number?, string?, number?),
	flush: (self: File) -> (File?, string?, number?),
	close: (self: File) -> (boolean?, string?, number?),
	setvbuf: (self: File, mode: "no" | "full" | "line", size: number?) -> boolean,
}

export type Metadata = {
	kind: "file" | "dir" | "symlink",
	size: number,
	readonly: boolean,
	modified: number?,
	created: number?,
	accessed: number?,
}

export type FS_API = {
	stdin: File,
	stdout: File,
	stderr: File,
	open: (path: string, mode: string?) -> (File?, string?, number?),
	lines: (path: string?, ...ReadFormat) -> () -> ...any,
	input: (file: (File | string)?) -> File,
	output: (file: (File | string)?) -> File,
	read: (...ReadFormat) -> ...any,
	write: (...(string | number)) -> (File?, string?, number?),
	close: (file: File?) -> (boolean?, string?, number?),
	type: (value: any) -> ("file" | "closed file")?,
	readFile: (path: string) -> string,
	writeFile: (path: string, contents: string | buffer) -> (),
	appendFile: (path: string, contents: string | buffer) -> (),
	readDir: (path: string) -> { string },
	makeDir: (path: string) -> (),
	remove: (path: string) -> (),
	removeDir: (path: string) -> (),
	rename: (from: string, to: string) -> (),
	copy: (from: string, to: string) -> (),
	exists: (path: string) -> boolean,
	isFile: (path: string) -> boolean,
	isDir: (path: string) -> boolean,
	metadata: (path: string) -> Metadata,
	tmpname: () -> string,
	tmpdir: () -> string,
}

export type Asset = BaseGameObject & {
	Path: string,
	Size: number,
	Extension: string?,
	ReadString: (self: Asset) -> string,
	ReadBuffer: (self: Asset) -> buffer,
	Open: (self: Asset) -> File,
}

export type Asset_API = {
	Load: (path: string) -> Asset,
	LoadString: (path: string) -> string,
	FromBytes: (name: string, data: Bytes) -> Asset,
	FromBase64: (name: string, text: string) -> Asset,
}

export type Dirs = {
	home: string?,
	appData: string?,
	localAppData: string?,
	config: string?,
	cache: string?,
	temp: string,
	documents: string?,
	desktop: string?,
	downloads: string?,
	pictures: string?,
	music: string?,
	videos: string?,
	game: string?,
	save: string?,
}

export type ShaderLanguage = "wgsl" | "glsl" | "spirv"

export type ShaderStage = "vertex" | "fragment" | "compute" | "task" | "mesh"

export type ShaderCombo = BaseGameObject & {
	Language: ShaderLanguage,
	Parts: { string },
	Source: string,
}

export type ShaderSource = string | buffer | Asset | File | ShaderCombo | {
	Source: string | buffer | Asset | File | ShaderCombo,
	Name: string?,
	Language: ShaderLanguage?,
	Stage: ShaderStage?,
}

export type ShaderComboOptions = {
	Name: string?,
	Language: ShaderLanguage?,
	Stage: ShaderStage?,
}

export type ShaderEntryPoint = {
	Name: string,
	Stage: string,
	WorkgroupSize: UDim,
}

export type Shader = BaseGameObject & {
	Language: ShaderLanguage,
	Compiled: boolean,
	Error: string?,
	Size: number,
	EntryPoints: { ShaderEntryPoint },
	GetSpirv: (self: Shader) -> buffer,
}

export type Shader_API = {
	Compile: ((source: ShaderSource) -> Shader)
		& ((sources: { ShaderSource }) -> { Shader })
		& ((sources: ShaderSource | { ShaderSource }, callback: (shader: Shader, index: number) -> ()) -> ()),
	Combine: (parts: { ShaderSource }, options: ShaderComboOptions?) -> ShaderCombo,
	Prelude: string,
}

export type ShaderValue =
	number
	| boolean
	| string
	| buffer
	| vector
	| UDim
	| Color
	| EnumItem
	| Asset
	| Renderable
	| { [any]: any }

export type Renderable = BaseGameObject & {
	SinkUpdates: boolean,
	ZIndex: number,
	BlendMode: BlendModeEnum,
	RenderHook: Pointer?,
	RenderHookData: Pointer?,
	LoadShader: (self: Renderable, shader: Shader) -> (),
	RemoveShader: (self: Renderable, shader: Shader) -> boolean,
	ClearShaders: (self: Renderable) -> (),
	HasShader: (self: Renderable, shader: Shader) -> boolean,
	GetShaders: (self: Renderable) -> { Shader },
	WriteShaderData: ((self: Renderable, shader: Shader, name: string, value: ShaderValue) -> ())
		& ((self: Renderable, shader: Shader, values: { [string]: ShaderValue }) -> ()),
	ReadShaderData: (self: Renderable, shader: Shader, name: string) -> any,
}

export type RenderableCustom = Renderable & {
	VertexCount: number,
	InstanceCount: number,
}

export type RenderablePlacement = {
	Position: UDim,
	Size: UDim,
	AnchorPoint: UDim,
	Rotation: number,
	Color: Color,
}

export type RenderableShape = Renderable & RenderablePlacement & {
	Shape: ShapeTypeEnum,
	Outline: { UDim }?,
	StrokeColor: Color,
	StrokeThickness: number,
}

export type RenderableImage = Renderable & RenderablePlacement & {
	Image: Asset,
	ImageSize: UDim,
	OffsetPosition: UDim,
	OffsetSize: UDim,
	ResampleMode: ResampleModeEnum,
	FlipX: boolean,
	FlipY: boolean,
	HitThreshold: number?,
}

export type RenderableText = Renderable & RenderablePlacement & {
	Font: Asset,
	Text: string,
	TextSize: number,
	Bold: boolean,
	Italic: boolean,
	Underline: boolean,
	Strikethrough: boolean,
	TextXAlignment: TextXAlignmentEnum,
	TextYAlignment: TextYAlignmentEnum,
	TextWrapped: boolean,
	LineHeight: number,
	LetterSpacing: number,
	BackgroundColor: Color,
	StrokeColor: Color,
	StrokeThickness: number,
	TextBounds: UDim,
}

export type RenderableClasses = {
	Renderable: RenderableCustom,
	RenderableShape: RenderableShape,
	RenderableImage: RenderableImage,
	RenderableText: RenderableText,
}

export type RenderableConfig = {
	Name: string?,
	SinkUpdates: boolean?,
	ZIndex: number?,
	BlendMode: BlendModeEnum?,
	Shaders: { Shader }?,
	RenderHook: Pointer?,
	RenderHookData: Pointer?,
}

export type RenderablePlacementConfig = {
	Position: UDim?,
	Size: UDim?,
	AnchorPoint: UDim?,
	Rotation: number?,
	Color: Color?,
}

export type RenderableCustomConfig = RenderableConfig & {
	VertexCount: number?,
	InstanceCount: number?,
}

export type RenderableShapeConfig = RenderableConfig & RenderablePlacementConfig & {
	Shape: ShapeTypeEnum?,
	Outline: { UDim }?,
	StrokeColor: Color?,
	StrokeThickness: number?,
}

export type RenderableImageConfig = RenderableConfig & RenderablePlacementConfig & {
	Image: Asset,
	OffsetPosition: UDim?,
	OffsetSize: UDim?,
	ResampleMode: ResampleModeEnum?,
	FlipX: boolean?,
	FlipY: boolean?,
	HitThreshold: number?,
}

export type RenderableTextConfig = RenderableConfig & RenderablePlacementConfig & {
	Font: Asset,
	Text: string?,
	TextSize: number?,
	Bold: boolean?,
	Italic: boolean?,
	Underline: boolean?,
	Strikethrough: boolean?,
	TextXAlignment: TextXAlignmentEnum?,
	TextYAlignment: TextYAlignmentEnum?,
	TextWrapped: boolean?,
	LineHeight: number?,
	LetterSpacing: number?,
	BackgroundColor: Color?,
	StrokeColor: Color?,
	StrokeThickness: number?,
}

export type RenderableConfigs = {
	Renderable: RenderableCustomConfig,
	RenderableShape: RenderableShapeConfig,
	RenderableImage: RenderableImageConfig,
	RenderableText: RenderableTextConfig,
}

export type RenderableAnyConfig = RenderableConfig & RenderablePlacementConfig & {
	Outline: { UDim }?,
	HitThreshold: number?,
	VertexCount: number?,
	InstanceCount: number?,
	Shape: ShapeTypeEnum?,
	StrokeColor: Color?,
	StrokeThickness: number?,
	Image: Asset?,
	OffsetPosition: UDim?,
	OffsetSize: UDim?,
	ResampleMode: ResampleModeEnum?,
	FlipX: boolean?,
	FlipY: boolean?,
	Font: Asset?,
	Text: string?,
	TextSize: number?,
	Bold: boolean?,
	Italic: boolean?,
	Underline: boolean?,
	Strikethrough: boolean?,
	TextXAlignment: TextXAlignmentEnum?,
	TextYAlignment: TextYAlignmentEnum?,
	TextWrapped: boolean?,
	LineHeight: number?,
	LetterSpacing: number?,
	BackgroundColor: Color?,
}

export type QueryParams = {
	Include: { Renderable }?,
	Exclude: { Renderable }?,
}

export type RaycastResult = {
	Renderable: Renderable,
	Position: UDim,
	Normal: UDim,
	Distance: number,
}

export type Renderable_API = {
	new: <K>(className: keyof<RenderableClasses> & K, config: RenderableAnyConfig?) -> index<RenderableClasses, K>,
	QueryPoint: (point: UDim, params: QueryParams?) -> { Renderable },
	QueryArea: ((center: UDim, size: UDim, rotation: number?, params: QueryParams?) -> { Renderable })
		& ((center: UDim, size: UDim, params: QueryParams) -> { Renderable }),
	QueryRadius: (center: UDim, radius: number, params: QueryParams?) -> { Renderable },
	Raycast: (origin: UDim, direction: UDim, params: QueryParams?) -> RaycastResult?,
	RaycastAll: (origin: UDim, direction: UDim, params: QueryParams?) -> { RaycastResult },
	WaitFor: <T...>(T...) -> T...,
	GetRenderables: () -> { Renderable },
}

export type Input_API = {
	IsConnected: boolean,
	ActivationChanged: Signal<boolean>,
	KeyDown: Signal<KeyCodeEnum>,
	KeyUp: Signal<KeyCodeEnum>,
	TextInput: Signal<string>,
	IsKeyDown: (self: Input_API, key: KeyCodeEnum) -> boolean,
	GetKeysDown: (self: Input_API) -> { KeyCodeEnum },
}

export type Mouse_API = {
	IsConnected: boolean,
	ActivationChanged: Signal<boolean>,
	Position: UDim,
	IsInside: boolean,
	Icon: MouseIconEnum,
	Visible: boolean,
	LockMode: MouseLockModeEnum,
	Moved: Signal<UDim, UDim>,
	ButtonDown: Signal<MouseButtonEnum, UDim>,
	ButtonUp: Signal<MouseButtonEnum, UDim>,
	Scrolled: Signal<UDim>,
	Entered: Signal<()>,
	Left: Signal<()>,
	IsButtonDown: (self: Mouse_API, button: MouseButtonEnum) -> boolean,
	GetButtonsDown: (self: Mouse_API) -> { MouseButtonEnum },
}

export type ControllerInfo = {
	Id: number,
	Name: string,
	CanVibrate: boolean,
}

export type Controller_API = {
	IsConnected: boolean,
	ActivationChanged: Signal<boolean>,
	ButtonDown: Signal<ControllerButtonEnum, number>,
	ButtonUp: Signal<ControllerButtonEnum, number>,
	AxisChanged: Signal<ControllerAxisEnum, number, number>,
	Connected: Signal<number, string>,
	Disconnected: Signal<number>,
	GetControllers: (self: Controller_API) -> { ControllerInfo },
	IsControllerConnected: (self: Controller_API, id: number) -> boolean,
	IsButtonDown: (self: Controller_API, button: ControllerButtonEnum, id: number?) -> boolean,
	GetButtonsDown: (self: Controller_API, id: number?) -> { ControllerButtonEnum },
	GetAxis: (self: Controller_API, axis: ControllerAxisEnum, id: number?) -> number,
	GetStick: (self: Controller_API, stick: ControllerStickEnum, id: number?) -> UDim,
	Vibrate: (self: Controller_API, strength: number, duration: number, id: number?) -> (),
	StopVibrating: (self: Controller_API, id: number?) -> (),
}

export type TouchPoint = {
	Id: number,
	Position: UDim,
}

export type Touch_API = {
	IsConnected: boolean,
	ActivationChanged: Signal<boolean>,
	Started: Signal<number, UDim, number?>,
	Moved: Signal<number, UDim, UDim, number?>,
	Ended: Signal<number, UDim>,
	Cancelled: Signal<number>,
	GetTouches: (self: Touch_API) -> { TouchPoint },
}

export type NodeObject = BaseGameObject

export type NodeInput = {
	Node: NodeObject,
	Link: (self: NodeInput, target: NodeOutput | NodeObject) -> boolean,
	Unlink: (self: NodeInput, target: (NodeOutput | NodeObject)?) -> boolean,
	IsLinked: (self: NodeInput, target: (NodeOutput | NodeObject)?) -> boolean,
	GetLinks: (self: NodeInput) -> { NodeOutput },
}

export type NodeOutput = {
	Node: NodeObject,
	Link: (self: NodeOutput, source: NodeInput | NodeObject) -> boolean,
	Unlink: (self: NodeOutput, source: (NodeInput | NodeObject)?) -> boolean,
	IsLinked: (self: NodeOutput, source: (NodeInput | NodeObject)?) -> boolean,
	GetLinks: (self: NodeOutput) -> { NodeInput },
}

export type SoundNode = NodeObject & {
	Input: NodeInput,
	Volume: number,
	PlaybackSpeed: number,
	Looping: boolean,
	LoopStart: number,
	LoopEnd: number,
	PlayPosition: number,
	Length: number,
	IsPlaying: boolean,
	IsPaused: boolean,
	SampleRate: number,
	Channels: number,
	Asset: string?,
	Started: Signal<()>,
	Stopped: Signal<()>,
	Paused: Signal<()>,
	Resumed: Signal<()>,
	Ended: Signal<()>,
	Looped: Signal<number>,
	Play: (self: SoundNode) -> (),
	Stop: (self: SoundNode) -> (),
	Pause: (self: SoundNode) -> (),
	Resume: (self: SoundNode) -> (),
	PlayOneShot: (self: SoundNode, volume: number?) -> (),
}

export type FromString = SoundNode

export type AudioPacket = {
	SampleRate: number,
	Channels: number,
	Frames: number,
	Duration: number,
	Format: AudioFormatEnum,
	Sequence: number,
	Peak: number,
	Loudness: number,
	ToString: (self: AudioPacket) -> string,
	ToBuffer: (self: AudioPacket) -> buffer,
	GetSamples: (self: AudioPacket, channel: number?) -> { number },
}

export type FromBytes = NodeObject & {
	Input: NodeInput,
	Volume: number,
	MaxBuffered: number,
	Prebuffer: number,
	SampleRate: number,
	Channels: number,
	Format: AudioFormatEnum,
	Buffered: number,
	IsPlaying: boolean,
	Drained: Signal<()>,
	Push: (self: FromBytes, data: string | buffer | AudioPacket) -> (),
	Clear: (self: FromBytes) -> (),
}

export type ToSpeaker = NodeObject & {
	Output: NodeOutput,
	Volume: number,
	Device: string?,
	OwnedByWindow: boolean,
	Spatial: boolean,
	Position: vector,
	Direction: vector,
	MinDistance: number,
	MaxDistance: number,
	RollOffMode: RollOffModeEnum,
	ConeInnerAngle: number,
	ConeOuterAngle: number,
	ConeOuterVolume: number,
	Binaural: boolean,
}

export type ToBytes = NodeObject & {
	Output: NodeOutput,
	Enabled: boolean,
	SampleRate: number,
	Channels: number,
	Format: AudioFormatEnum,
	PacketDuration: number,
	SkipSilence: boolean,
	OnIncoming: Signal<AudioPacket>,
}

export type SoundModifier = NodeObject & {
	Input: NodeInput,
	Output: NodeOutput,
	Enabled: boolean,
}

export type GainModifier = SoundModifier & {
	Volume: number,
	Fade: (self: GainModifier, volume: number, duration: number) -> (),
}

export type PanModifier = SoundModifier & {
	Pan: number,
}

export type LowPassModifier = SoundModifier & {
	Cutoff: number,
	Resonance: number,
}

export type HighPassModifier = SoundModifier & {
	Cutoff: number,
	Resonance: number,
}

export type BandPassModifier = SoundModifier & {
	Frequency: number,
	Q: number,
}

export type NotchModifier = SoundModifier & {
	Frequency: number,
	Q: number,
}

export type PeakModifier = SoundModifier & {
	Frequency: number,
	Q: number,
	Gain: number,
}

export type LowShelfModifier = SoundModifier & {
	Frequency: number,
	Gain: number,
}

export type HighShelfModifier = SoundModifier & {
	Frequency: number,
	Gain: number,
}

export type EqualizerModifier = SoundModifier & {
	LowGain: number,
	MidGain: number,
	HighGain: number,
	LowFrequency: number,
	HighFrequency: number,
}

export type EchoModifier = SoundModifier & {
	Delay: number,
	Feedback: number,
	Mix: number,
	PingPong: boolean,
}

export type ReverbModifier = SoundModifier & {
	RoomSize: number,
	Damping: number,
	Width: number,
	Mix: number,
	PreDelay: number,
}

export type ChorusModifier = SoundModifier & {
	Rate: number,
	Depth: number,
	Mix: number,
}

export type FlangerModifier = SoundModifier & {
	Rate: number,
	Depth: number,
	Feedback: number,
	Mix: number,
}

export type PhaserModifier = SoundModifier & {
	Rate: number,
	Depth: number,
	Feedback: number,
	Mix: number,
}

export type TremoloModifier = SoundModifier & {
	Rate: number,
	Depth: number,
}

export type VibratoModifier = SoundModifier & {
	Rate: number,
	Depth: number,
}

export type DistortionModifier = SoundModifier & {
	Drive: number,
	Tone: number,
	Mix: number,
}

export type BitCrusherModifier = SoundModifier & {
	Bits: number,
	Downsample: number,
	Mix: number,
}

export type CompressorModifier = SoundModifier & {
	Threshold: number,
	Ratio: number,
	Attack: number,
	Release: number,
	MakeupGain: number,
}

export type LimiterModifier = SoundModifier & {
	Threshold: number,
	Release: number,
}

export type NoiseGateModifier = SoundModifier & {
	Threshold: number,
	Attack: number,
	Release: number,
	Hold: number,
}

export type PitchShiftModifier = SoundModifier & {
	Pitch: number,
}

export type RingModulatorModifier = SoundModifier & {
	Frequency: number,
	Mix: number,
}

export type StereoWidthModifier = SoundModifier & {
	Width: number,
}

export type MeterModifier = SoundModifier & {
	Peak: number,
	Loudness: number,
}

export type SoundModifiers = {
	Gain: GainModifier,
	Pan: PanModifier,
	LowPass: LowPassModifier,
	HighPass: HighPassModifier,
	BandPass: BandPassModifier,
	Notch: NotchModifier,
	Peak: PeakModifier,
	LowShelf: LowShelfModifier,
	HighShelf: HighShelfModifier,
	Equalizer: EqualizerModifier,
	Echo: EchoModifier,
	Reverb: ReverbModifier,
	Chorus: ChorusModifier,
	Flanger: FlangerModifier,
	Phaser: PhaserModifier,
	Tremolo: TremoloModifier,
	Vibrato: VibratoModifier,
	Distortion: DistortionModifier,
	BitCrusher: BitCrusherModifier,
	Compressor: CompressorModifier,
	Limiter: LimiterModifier,
	NoiseGate: NoiseGateModifier,
	PitchShift: PitchShiftModifier,
	RingModulator: RingModulatorModifier,
	StereoWidth: StereoWidthModifier,
	Meter: MeterModifier,
}

export type SoundListener = {
	Position: vector,
	Forward: vector,
	Up: vector,
}

export type SoundNodeConfig = {
	Name: string?,
	Volume: number?,
	PlaybackSpeed: number?,
	Looping: boolean?,
	LoopStart: number?,
	LoopEnd: number?,
	PlayPosition: number?,
}

export type FromBytesConfig = {
	Name: string?,
	Volume: number?,
	MaxBuffered: number?,
	Prebuffer: number?,
	SampleRate: number?,
	Channels: number?,
	Format: AudioFormatEnum?,
}

export type ToSpeakerConfig = {
	Name: string?,
	Volume: number?,
	Device: string?,
	OwnedByWindow: boolean?,
	Spatial: boolean?,
	Position: vector?,
	Direction: vector?,
	MinDistance: number?,
	MaxDistance: number?,
	RollOffMode: RollOffModeEnum?,
	ConeInnerAngle: number?,
	ConeOuterAngle: number?,
	ConeOuterVolume: number?,
	Binaural: boolean?,
}

export type ToBytesConfig = {
	Name: string?,
	Enabled: boolean?,
	SampleRate: number?,
	Channels: number?,
	Format: AudioFormatEnum?,
	PacketDuration: number?,
	SkipSilence: boolean?,
}

export type Sound_API = {
	Volume: number,
	Listener: SoundListener,
	SampleRate: number,
	DefaultDevice: string?,
	IsConnected: boolean,
	ActivationChanged: Signal<boolean>,
	SoundNode: (self: Sound_API, asset: Asset | string, config: SoundNodeConfig?) -> SoundNode,
	FromString: (self: Sound_API, data: string | buffer, config: SoundNodeConfig?) -> FromString,
	FromBytes: (self: Sound_API, config: FromBytesConfig?) -> FromBytes,
	ToSpeaker: (self: Sound_API, config: ToSpeakerConfig?) -> ToSpeaker,
	ToBytes: (self: Sound_API, config: ToBytesConfig?) -> ToBytes,
	Modifier: <K>(self: Sound_API, kind: keyof<SoundModifiers> & K, config: { [string]: any }?) -> index<SoundModifiers, K>,
	GetNodes: (self: Sound_API) -> { NodeObject },
	StopAll: (self: Sound_API) -> (),
	PauseAll: (self: Sound_API) -> (),
	ResumeAll: (self: Sound_API) -> (),
	GetDevices: (self: Sound_API) -> { string },
	DeviceExists: (self: Sound_API, name: string) -> boolean,
}

export type WindowAPIs = {
	Renderable: Renderable_API,
	Input: Input_API,
	Mouse: Mouse_API,
	Controller: Controller_API,
	Touch: Touch_API,
	Sound: Sound_API,
}

export type PostProcess = BaseGameObject & {
	Enabled: boolean,
	Order: number,
	RenderHook: Pointer?,
	RenderHookData: Pointer?,
	LoadShader: (self: PostProcess, shader: Shader) -> (),
	RemoveShader: (self: PostProcess, shader: Shader) -> boolean,
	ClearShaders: (self: PostProcess) -> (),
	HasShader: (self: PostProcess, shader: Shader) -> boolean,
	GetShaders: (self: PostProcess) -> { Shader },
	WriteShaderData: ((self: PostProcess, shader: Shader, name: string, value: ShaderValue) -> ())
		& ((self: PostProcess, shader: Shader, values: { [string]: ShaderValue }) -> ()),
	ReadShaderData: (self: PostProcess, shader: Shader, name: string) -> any,
}

export type WindowConfig = {
	Title: string?,
	Size: UDim?,
	Position: UDim?,
	SizeLocked: boolean?,
	FPS: number?,
	Type: WindowTypeEnum?,
	Resizable: boolean?,
	Icon: (Asset | string)?,
	BackgroundColor: Color?,
}

export type Window = BaseGameObject & {
	Title: string,
	Size: UDim,
	Position: UDim,
	SizeLocked: boolean,
	FPS: number,
	Type: WindowTypeEnum,
	Resizable: boolean,
	Icon: (Asset | string)?,
	BackgroundColor: Color,
	Focused: boolean,
	IsOpen: boolean,
	SizeChanged: Signal<UDim>,
	WindowUpdate: Signal<UDim>,
	Moved: Signal<UDim>,
	PreFrame: Signal<number>,
	OnFrame: Signal<number>,
	AfterFrame: Signal<number>,
	FocusGained: Signal<()>,
	FocusLost: Signal<()>,
	Closed: Signal<()>,
	GetAPI: <K>(self: Window, name: keyof<WindowAPIs> & K) -> index<WindowAPIs, K>,
	AddPostProcess: (self: Window, shader: Shader?) -> PostProcess,
	GetPostProcesses: (self: Window) -> { PostProcess },
	ClearPostProcesses: (self: Window) -> (),
	Close: (self: Window) -> (),
}

export type Window_API = {
	new: (config: WindowConfig?) -> Window,
}

export type ProcessOptions = {
	cwd: string?,
	env: { [string]: string }?,
	clearEnv: boolean?,
	shell: boolean?,
	stdin: string?,
	stdio: ("capture" | "inherit")?,
}

export type ProcessStatus = {
	ok: boolean,
	code: number,
}

export type ProcessResult = ProcessStatus & {
	stdout: string,
	stderr: string,
}

export type Child = BaseGameObject & {
	Pid: number?,
	Stdin: File?,
	Stdout: File?,
	Stderr: File?,
	Wait: (self: Child) -> ProcessStatus,
	Kill: (self: Child) -> (),
}

export type Process_API = {
	Heartbeat: Signal<number>,
	BindToClose: (callback: () -> ()) -> (),
	dirs: Dirs,
	executable: string?,
	gameName: string,
	args: { string },
	env: { [string]: string },
	os: string,
	arch: string,
	pid: number,
	cwd: () -> string,
	exit: (code: (number | boolean)?) -> never,
	spawn: (program: string, args: { string }?, options: ProcessOptions?) -> ProcessResult,
	start: (program: string, args: { string }?, options: ProcessOptions?) -> Child,
}

export type SerdeFormat = "json" | "jsonc" | "toml" | "yaml"

export type Serde_API = {
	Encode: (format: SerdeFormat, value: any, pretty: boolean?) -> string,
	Decode: (format: SerdeFormat, text: string) -> any,
}

export type Signal_API = {
	new: <T...>() -> Signal<T...>,
}

export type Messenger_API = Messenger

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | string

export type HttpRequest = {
	Url: string,
	Method: HttpMethod?,
	Headers: { [string]: string | number }?,
	Body: (string | buffer)?,
	Timeout: number?,
}

export type HttpResponse = {
	StatusCode: number,
	StatusMessage: string,
	Ok: boolean,
	Url: string,
	Headers: { [string]: string },
	Body: string,
}

export type TcpSocket = BaseGameObject & {
	IsOpen: boolean,
	RemoteHost: string,
	RemotePort: number,
	LocalPort: number,
	Received: Signal<string>,
	Closed: Signal<string>,
	Send: (self: TcpSocket, data: string | buffer) -> (),
	Close: (self: TcpSocket) -> (),
}

export type TcpServer = BaseGameObject & {
	IsOpen: boolean,
	Host: string,
	Port: number,
	Connected: Signal<TcpSocket>,
	Closed: Signal<()>,
	Close: (self: TcpServer) -> (),
}

export type UdpSocket = BaseGameObject & {
	IsOpen: boolean,
	Host: string,
	Port: number,
	Received: Signal<string, string, number>,
	Closed: Signal<()>,
	Send: (self: UdpSocket, data: string | buffer, host: string, port: number) -> (),
	Close: (self: UdpSocket) -> (),
}

export type WebSocket = BaseGameObject & {
	IsOpen: boolean,
	Received: Signal<string, boolean>,
	Closed: Signal<number, string>,
	Send: (self: WebSocket, message: string | buffer, binary: boolean?) -> (),
	Close: (self: WebSocket, code: number?, reason: string?) -> (),
}

export type WebSocketServer = BaseGameObject & {
	IsOpen: boolean,
	Host: string,
	Port: number,
	Connected: Signal<WebSocket>,
	Closed: Signal<()>,
	Close: (self: WebSocketServer) -> (),
}

export type Net_API = {
	Request: (request: HttpRequest) -> HttpResponse,
	Get: (url: string, headers: { [string]: string | number }?) -> HttpResponse,
	Post: (url: string, body: string | buffer, headers: { [string]: string | number }?) -> HttpResponse,
	TcpConnect: (host: string, port: number) -> TcpSocket,
	TcpListen: (port: number?, host: string?) -> TcpServer,
	UdpBind: (port: number?, host: string?) -> UdpSocket,
	WebSocketConnect: (url: string, headers: { [string]: string | number }?) -> WebSocket,
	WebSocketListen: (port: number?, host: string?) -> WebSocketServer,
}

export type Random = {
	Seed: number,
	NextInt: (self: Random, min: number, max: number) -> number,
	NextNumber: (self: Random, min: number?, max: number?) -> number,
	NextBool: (self: Random, chance: number?) -> boolean,
	NextSign: (self: Random) -> number,
	NextGaussian: (self: Random, mean: number?, deviation: number?) -> number,
	NextExponential: (self: Random, rate: number?) -> number,
	NextAngle: (self: Random) -> number,
	NextDirection: (self: Random) -> UDim,
	NextUnitVector: (self: Random) -> UDim,
	NextUDim: (self: Random, min: UDim, max: UDim) -> UDim,
	NextPointInCircle: (self: Random, center: UDim, radius: number) -> UDim,
	NextPointOnCircle: (self: Random, center: UDim, radius: number) -> UDim,
	NextColor: (self: Random, alpha: number?) -> Color,
	NextHue: (self: Random, saturation: number?, value: number?) -> Color,
	NextBytes: (self: Random, count: number) -> string,
	NextString: (self: Random, length: number, characters: string?) -> string,
	NextUUID: (self: Random) -> string,
	Pick: <T>(self: Random, list: { T }) -> (T?, number?),
	WeightedPick: <T>(self: Random, list: { T }, weights: { number }) -> (T, number),
	Shuffle: <T>(self: Random, list: { T }) -> { T },
	Sample: <T>(self: Random, list: { T }, count: number) -> { T },
	Noise: (self: Random, position: UDim) -> number,
	FractalNoise: (self: Random, position: UDim, octaves: number?, persistence: number?, lacunarity: number?) -> number,
	Clone: (self: Random) -> Random,
	Reset: (self: Random) -> (),
}

export type Random_API = {
	new: (seed: (number | string)?) -> Random,
}

export type Bytes = string | buffer

export type Hasher = {
	Algorithm: HashAlgorithmEnum,
	Update: (self: Hasher, data: Bytes) -> (),
	Finish: (self: Hasher) -> string,
}

export type KeyPair = {
	PublicKey: string,
	PrivateKey: string,
}

export type Crypto_API = {
	Hash: (algorithm: HashAlgorithmEnum, data: Bytes) -> string,
	Hasher: (algorithm: HashAlgorithmEnum) -> Hasher,
	Hmac: (algorithm: HashAlgorithmEnum, key: Bytes, data: Bytes) -> string,
	Hkdf: (algorithm: HashAlgorithmEnum, secret: Bytes, salt: Bytes?, info: Bytes?, length: number) -> string,
	Pbkdf2: (algorithm: HashAlgorithmEnum, password: Bytes, salt: Bytes, iterations: number, length: number) -> string,
	HashPassword: (password: Bytes) -> string,
	VerifyPassword: (password: Bytes, hash: string) -> boolean,
	RandomBytes: (count: number) -> string,
	RandomUUID: () -> string,
	GenerateKey: (algorithm: CipherAlgorithmEnum) -> string,
	Encrypt: (algorithm: CipherAlgorithmEnum, key: Bytes, data: Bytes, associatedData: Bytes?) -> string,
	Decrypt: (algorithm: CipherAlgorithmEnum, key: Bytes, data: Bytes, associatedData: Bytes?) -> string?,
	GenerateKeyPair: (algorithm: KeyAlgorithmEnum) -> KeyPair,
	GetPublicKey: (algorithm: KeyAlgorithmEnum, privateKey: Bytes) -> string,
	Sign: (algorithm: KeyAlgorithmEnum, privateKey: Bytes, message: Bytes) -> string,
	Verify: (algorithm: KeyAlgorithmEnum, publicKey: Bytes, message: Bytes, signature: Bytes) -> boolean,
	SharedSecret: (algorithm: KeyAlgorithmEnum, privateKey: Bytes, peerPublicKey: Bytes) -> string,
	Equals: (first: Bytes, second: Bytes) -> boolean,
	ToHex: (data: Bytes) -> string,
	FromHex: (text: string) -> string,
	ToBase64: (data: Bytes, urlSafe: boolean?) -> string,
	FromBase64: (text: string) -> string,
}

export type DLLTypeName =
	"void"
	| "bool"
	| "i8"
	| "u8"
	| "i16"
	| "u16"
	| "i32"
	| "u32"
	| "i64"
	| "u64"
	| "isize"
	| "usize"
	| "f32"
	| "f64"
	| "pointer"
	| "string"
	| "wstring"
	| "char"
	| "uchar"
	| "short"
	| "ushort"
	| "int"
	| "uint"
	| "long"
	| "ulong"
	| "longlong"
	| "ulonglong"
	| "float"
	| "double"
	| "size_t"
	| "ssize_t"
	| "intptr_t"
	| "uintptr_t"

export type DLLType = DLLTypeName | StructType | ArrayType

export type StructField = { any } | { Name: string, Type: DLLType }

export type StructType = {
	Size: number,
	Alignment: number,
	Fields: { string },
	Offset: (self: StructType, field: string) -> number,
	New: (self: StructType, values: { [string]: any }?) -> Pointer,
}

export type ArrayType = {
	Size: number,
	Alignment: number,
	Length: number,
	New: (self: ArrayType, values: { any }?) -> Pointer,
}

export type Pointer = setmetatable<{
	Address: number,
	IsNull: boolean,
	Size: number?,
	Read: (self: Pointer, valueType: DLLType, offset: number?) -> any,
	Write: (self: Pointer, valueType: DLLType, value: any, offset: number?) -> (),
	ReadString: (self: Pointer, length: number?, offset: number?) -> string,
	ReadWideString: (self: Pointer, length: number?, offset: number?) -> string,
	WriteString: (self: Pointer, text: string, offset: number?) -> (),
	WriteWideString: (self: Pointer, text: string, offset: number?) -> (),
	ReadBuffer: (self: Pointer, length: number, offset: number?) -> buffer,
	WriteBuffer: (self: Pointer, data: Bytes, offset: number?) -> (),
	Offset: (self: Pointer, bytes: number) -> Pointer,
	Free: (self: Pointer) -> (),
}, PointerMetatable>

export type PointerMetatable = {
	__eq: (Pointer, Pointer) -> boolean,
	__tostring: (Pointer) -> string,
}

export type NativeFunctionOptions = {
	ErrorCode: boolean?,
	Parallel: boolean?,
}

export type NativeFunction = BaseGameObject & {
	Pointer: Pointer,
	Call: (self: NativeFunction, ...any) -> ...any,
} & ((...any) -> ...any)

export type Library = BaseGameObject & {
	Path: string,
	Exports: { [string]: any },
	GetServices: (self: Library) -> { string },
	HasSymbol: (self: Library, name: string) -> boolean,
	GetSymbol: (self: Library, name: string) -> Pointer,
	GetFunction: (
		self: Library,
		name: string,
		returnType: DLLType,
		argumentTypes: { DLLType }?,
		options: NativeFunctionOptions?
	) -> NativeFunction,
}

export type Callback = BaseGameObject & {
	Pointer: Pointer,
}

export type NativeHandle = Pointer | NativeFunction | Callback

export type DLL_API = {
	Load: (path: string) -> Library,
	Function: (
		pointer: NativeHandle,
		returnType: DLLType,
		argumentTypes: { DLLType }?,
		options: NativeFunctionOptions?
	) -> NativeFunction,
	Callback: (returnType: DLLType, argumentTypes: { DLLType }?, handler: (...any) -> any) -> Callback,
	Struct: (fields: { StructField }) -> StructType,
	Array: (element: DLLType, length: number) -> ArrayType,
	Alloc: (size: number) -> Pointer,
	New: (valueType: DLLType, value: any?) -> Pointer,
	String: (text: string, wide: boolean?) -> Pointer,
	Pointer: (address: number) -> Pointer,
	Null: Pointer,
	SizeOf: (valueType: DLLType) -> number,
	AlignOf: (valueType: DLLType) -> number,
	Copy: (destination: Pointer, source: Pointer, size: number) -> (),
	Fill: (destination: Pointer, value: number, size: number) -> (),
	Extension: string,
}

export type ContainerLibrary = BaseGameObject & {
	Id: string,
	Version: string,
	Description: string,
	Main: string,
	Path: string,
	Natives: { string },
	GetRequire: (self: ContainerLibrary) -> string,
}

export type Container_API = {
	Exists: (name: string) -> boolean,
	IsLoaded: (name: string) -> boolean,
	GetContainers: () -> { string },
	GetLibrary: (name: string) -> ContainerLibrary?,
	LoadLibrary: (name: string) -> ContainerLibrary,
	Refresh: () -> { string },
}

export type Screen = {
	Name: string,
	Position: UDim,
	Size: UDim,
	PixelSize: UDim,
	WorkPosition: UDim,
	WorkSize: UDim,
	Scale: number,
	RefreshRate: number?,
	IsPrimary: boolean,
}

export type Viewport_API = {
	GetScreens: () -> { Screen },
	GetPrimaryScreen: () -> Screen,
	GetScreenSize: () -> UDim,
	GetWindowScreen: (window: Window) -> Screen?,
}

export type Imports = {
	Asset: Asset_API,
	Bulk: Bulk_API,
	Container: Container_API,
	Crypto: Crypto_API,
	DLL: DLL_API,
	FS: FS_API,
	Messenger: Messenger_API,
	Net: Net_API,
	Process: Process_API,
	Random: Random_API,
	Serde: Serde_API,
	Shader: Shader_API,
	Signal: Signal_API,
	Viewport: Viewport_API,
	Window: Window_API,
}

export type ExternalModule = BaseGameObject & {
	Source: string,
	Folder: string,
	Entry: string,
	Files: number,
	IsLoaded: boolean,
	GetFiles: (self: ExternalModule) -> { string },
	Fetch: (self: ExternalModule) -> any,
	Drop: (self: ExternalModule) -> boolean,
}

declare import: <K>(name: keyof<Imports> & K) -> index<Imports, K>
declare function ecall(path: string): ExternalModule
declare function SetGlobal(name: string, value: any): ()

declare enum: Enums
declare udim: UDim_API
declare color: Color_API

declare function EnterParallel(): ()
declare function ExitParallel(): ()
