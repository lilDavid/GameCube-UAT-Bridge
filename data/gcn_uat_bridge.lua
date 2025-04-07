---@meta


---@alias Type
---| '"integer"'  # Unsigned integer
---| '"unsigned"'  # Unsigned integer
---| '"signed"'  # Signed integer
---| '"float"'  # Floating point number
---| '"string"'  # Byte sequence
---| '"bytes"'  # Byte sequence
---| nil  # Byte sequence

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
GameCube.BaseAddress = nil

---Read a value from an address in memory
---@param address integer
---@param size integer
---@param type Type?  # Default bytes
---@return integer|number|string|nil
function GameCube:ReadAddress(address, size, type) end

---Follow a pointer at an address, and read the value offset from the result
---@param address integer
---@param size integer
---@param offset integer
---@param type Type?  # Default bytes
function GameCube:ReadPointer(address, size, offset, type) end

---Follow a chain of pointers to pointers and read the final result
---@param address integer
---@param size integer
---@param offsets integer[]
---@param type Type?  # Default bytes
function GameCube:ReadPointerChain(address, size, offsets, type) end


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
