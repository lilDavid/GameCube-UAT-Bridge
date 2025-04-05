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

---@return GameInterface
function ScriptHost:CreateGameInterface() end

---@param name string
---@param interface GameInterface
function ScriptHost:AddGameInterface(name, interface) end


---@class GameCube
GameCube = {}

---@type integer
GameCube.BaseAddress = nil

---@param address integer
---@param size integer
---@param type Type?  # Default bytes
---@return integer|number|string|nil
function GameCube:ReadAddress(address, size, type) end

---@param address integer
---@param size integer
---@param offsets integer[]
---@param type Type?  # Default bytes
function GameCube:ReadPointerChain(address, size, offsets, type) end


---@class VariableStore
VariableStore = {}

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

---@type fun(self:GameInterface):boolean
GameInterface.VerifyFunc = nil

---@type fun(self:GameInterface, store:VariableStore)
GameInterface.GameWatcher = nil
