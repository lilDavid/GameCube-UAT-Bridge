---@meta


---@alias TypeSpecifier
---| '"u8"'  # Unsigned byte
---| '"s8"'  # Signed byte
---| '"u16"'  # Unsigned short
---| '"s16"'  # Signed short
---| '"u32"'  # Unsigned int
---| '"s32"'  # Signed int
---| '"s64"'  # Signed long. Unsigned is not supported.
---| '"f32"'  # Single precision float
---| '"f64"'  # Double precision float
---| integer  # Number of bytes. Nonpositive numbers will return nil.

---@alias AnyValue
---| boolean
---| number
---| string
---| table
---| nil


---@class ScriptHost
ScriptHost = {}

---Create a new GameInterface
---@return GameInterface
function ScriptHost:CreateGameInterface() end

---Regester a GameInterface to read the GameCube's memory
---@param name string
---@param interface GameInterface
function ScriptHost:AddGameInterface(name, interface) end


---@class GameCube
GameCube = {}

---@type integer
GameCube.GameIDAddress = nil

---Read a single value from an address in memory. Prefer to read multiple values at once with GameCube:ReadBatch() if
---you can, as each call of either method can be slow.
---@param address integer
---@param type TypeSpecifier
---@param offset integer|nil  # If not nil, dereference the address using this offset
---@return integer|number|string|nil
function GameCube:ReadSingle(address, type, offset) end

---Read a batch of values from memory. Prefer to use this method if you can, as each call of either method can be slow.
---@param read_list [integer, TypeSpecifier, integer|nil][]  # Address, type, and offset of value
---@return (integer|number|string|nil)[]
function GameCube:Read(read_list) end


---@class VariableStore
VariableStore = {}

---Submit a variable to be sent to the tracker.
---@param name string
---@param value AnyValue
function VariableStore:WriteVariable(name, value) end


---@class GameInterface
GameInterface = {}

---@type string?
GameInterface.Name = nil

---@type string?
GameInterface.Version = nil

---@type string[]?
GameInterface.Features = nil

---@type string[]?
GameInterface.Slots = nil

---Called to determine if this interface can track the currently running game.
---Return true to accept, and false to reject.
---This method will be called repeatedly to ensure the correct game is still
---running.
---@type fun(self:GameInterface):boolean
GameInterface.VerifyFunc = nil

---Called to obtain the tracked variables from memory. Submit the variables read
---using the variable store's WriteVariable() method.
---@type fun(self:GameInterface, store:VariableStore)
GameInterface.GameWatcher = nil
