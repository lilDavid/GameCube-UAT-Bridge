# GameCube UAT Bridge

This program implements the [UAT protocol](https://github.com/black-sliver/UAT/blob/master/PROTOCOL.md) to enable
autotracking GameCube games in [PopTracker](https://github.com/black-sliver/PopTracker). This program supports
connecting to the multiworld fork of [Nintendont](https://github.com/randovania/Nintendont/releases/) on all platforms
and [Dolphin](https://dolphin-emu.org/) on Windows.

## Usage

This program uses Lua scripts to connect to the game and send variables to PopTracker. A minimal example script is as
follows:

```lua
ITEM_ID_MAPPING = {
    ["Ice Beam"] = 1,
    ["Wave Beam"] = 2,
    ["Plasma Beam"] = 3,
    ["Missile"] = 4,
    ["Scan Visor"] = 5,
    ["Morph Ball Bomb"] = 6,
    -- And so on
}

local metroid_prime_interface = ScriptHost:CreateGameInterface()
-- The game name and version are purely informational
metroid_prime_interface.Name = "Metroid Prime"
metroid_prime_interface.Version = "0-00"

metroid_prime_interface.VerifyFunc = function(self)
    local game_id = GameCube:ReadAddress(GameCube.BaseAddress, 6, "string")
    local revision = GameCube:ReadAddress(GameCube.BaseAddress + 6, 1, "integer")
    return game_id == "GM8E01" and revision == 0
end

metroid_prime_interface.GameWatcher = function(self, store)
    local player_state_address = GameCube:ReadPointer(0x8045AA60, 4, 0, "integer")
    if player_state_address then
        local inventory_table_address = player_state_address + 40
        for name, id in pairs(ITEM_ID_MAPPING) do
            local result = GameCube:ReadAddress(inventory_table_address + 8 * id + 4, 4, "integer")
            store:WriteVariable("inventory/" .. name, result)
        end
    end
end

ScriptHost:AddGameInterface("MetroidPrimeAP", metroid_prime_interface)
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
- Extend or change the Lua API to work with groups of simultaneous reads rather than pointer dereferences because
sequential reads from Nintendont are *expensive*
- Possibly change to an async API on the Rust, Lua, or both sides to better handle potentially blocking operations (And
manage Nintendont latency. Seriously, the scripts I've written to test this take north of a second to read their data.)
