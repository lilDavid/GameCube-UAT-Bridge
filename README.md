# GameCube UAT Bridge

This program implements the [UAT protocol](https://github.com/black-sliver/UAT/blob/master/PROTOCOL.md) to enable
autotracking GameCube games in [PopTracker](https://github.com/black-sliver/PopTracker). This program supports
connecting to the multiworld fork of [Nintendont](https://github.com/randovania/Nintendont/releases/) on all platforms
and [Dolphin](https://dolphin-emu.org/) on Windows.

## Usage

This program uses Lua scripts to connect to the game and send variables to PopTracker. A minimal example script is as
follows:

```lua
local LEVEL_MAPPING = {
    [972217896] = "Tallon Overworld",
    [2214002543] = "Chozo Ruins",
    -- And so on
}

ITEM_ID_MAPPING = {
    [1] = "Ice Beam",
    [2] = "Wave Beam",
    [3] = "Plasma Beam",
    [4] = "Missile",
    [5] = "Scan Visor",
    [6] = "Morph Ball Bomb",
    -- And so on
}

local metroid_prime_interface = ScriptHost:CreateGameInterface()
-- The game name and version are purely informational
metroid_prime_interface.Name = "Metroid Prime"
metroid_prime_interface.Version = "0-00"

metroid_prime_interface.VerifyFunc = function(self)
    local game_id, revision = table.unpack(GameCube:Read({
        {GameCube.GameIDAddress, 6}, -- Pass a number to get a string of that length
        {GameCube.GameIDAddress + 7, "u8"} -- Name a type to get that type back
    }))
    return game_id == "GM8E01" and revision == 0
end

metroid_prime_interface.GameWatcher = function(self, store)
    local player_state_address, world_id = table.unpack(GameCube:Read({
        -- Optional 3rd parameter is an offset to dereference a pointer at the given address
        {0x8045AA60, "u32", 0}, -- Read the value at 0x8045AA60, and get an int from the result
        {0x805A8C40, "u32", 0x84}, -- Read the value at 0x805A8C40, add 0x84, and get an int from the result
    }))

    if player_state_address then
        local inventory_table_address = player_state_address + 40
        -- Try to read as many values as you can at the same time because GameCube:Read() and GameCube:ReadSingle() are slow
        local read_list = {}
        local variables = {}
        for name, id in pairs(ITEM_ID_MAPPING) do
            table.insert(read_list, {inventory_table_address + 8 * id + 4, "u32"})
            table.insert(variables, name)
        end
        local result = GameCube:Read(read_list)
        for i, var in ipairs(variables) do
            store:WriteVariable("inventory/" .. var, result[i])
        end
    end

    local world = LEVEL_MAPPING[world_id]
    store:WriteVariable("Current Area", world)
end

-- The interface name can be whatever you want, but it helps to be descriptive
ScriptHost:AddGameInterface("MetroidPrime-YourName", metroid_prime_interface)
```

To run the server, start the program on the command line, pass in your Wii's IP address or "dolphin" on the command
line, and then pass paths to any connector scripts you want it to try:

```sh
./gamecube_uat_bridge '192.168.1.131' metroid_prime_connector.lua wind_waker_connector.lua
```

```ps1
.\gamecube_uat_bridge.exe dolphin metroid_prime_connector.lua
```

## Building

`cd` into the git repository and run `cargo build`.

## Future goals

- Eventually make this into a GUI app that minimizes into the system tray so that you may choose the connection type
and load scripts dynamically.
- Possibly change to an async API on the Rust, Lua, or both sides to better handle potentially blocking operations
