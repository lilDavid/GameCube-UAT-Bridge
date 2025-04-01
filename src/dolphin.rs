use std::error::Error;

use dolphin_memory::Dolphin;

use crate::gamecube::*;

pub fn read_game_world() -> Result<u32, Box<dyn Error>> {
    let dolphin = Dolphin::new()?;

    let game_id = dolphin.read_string(6, GCN_GAME_ID_ADDRESS as usize, None)?;
    let game_revision = dolphin.read_u8(GCN_GAME_ID_ADDRESS as usize + 6, None)?;
    println!(">> Game ID: {}", game_id);
    println!(">> Revision: {}", game_revision);

    let world = dolphin.read_u32(PRIME_GAME_STATE_ADDRESS as usize, Some(&[PRIME_WORLD_OFFSET as usize]))?;
    println!(">> Game world: {}", world);

    Ok(world)
}
