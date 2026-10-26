//! Terrain: the materials, a dry run, and a mound added high above the
//! first placed entity and dug away again.

use enshwick::terrain::{Shape, Terrain};
use enshwick::world::World;
use enshwick::{Load, Off};

/// How far above the placed entity the probe works, in metres: in the air.
const ABOVE: f64 = 40.0;

/// The terrain checks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TerrainChecks {
    terrain: Terrain,
    world: World,
}

impl TerrainChecks {
    pub(crate) fn new(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self {
            terrain: load.terrain(),
            world: load.world()?,
        })
    }

    pub(crate) fn run(self) {
        let materials = self.terrain.materials();
        let Some(material) = materials.first().map(|m| m.id) else {
            enshwick::warn!("FAIL terrain materials: none");
            return;
        };
        enshwick::info!("PASS terrain materials: {}", materials.len());
        let Some([x, y, z]) = self
            .world
            .entities(self.world.transform_type())
            .first()
            .map(|placed| placed.position)
        else {
            enshwick::info!("SKIP terrain edits: no placed entity to work near");
            return;
        };
        match self.terrain.dry().dig([x, y, z], 2.0, Shape::Round, &[]) {
            Ok(volume) => {
                enshwick::info!("PASS terrain dry dig at {x:.0},{y:.0},{z:.0}: {volume:.2} m³");
            }
            Err(why) => enshwick::warn!("FAIL terrain dry dig: {why}"),
        }
        let up = [x, y + ABOVE, z];
        let added = match self.terrain.add(up, 2.0, 1.0, material, Shape::Round) {
            Ok(volume) if volume > 0.0 => volume,
            other => {
                enshwick::warn!("FAIL terrain add: {other:?}");
                return;
            }
        };
        match self.terrain.dig(up, 4.0, Shape::Round, &[]) {
            Ok(removed) if removed > 0.0 => enshwick::info!(
                "PASS terrain added {added:.2} m³ of material {material} and dug {removed:.2} m³ away"
            ),
            other => enshwick::warn!("FAIL terrain dig after add: {other:?}"),
        }
        match self.terrain.dig(up, 100.0, Shape::Round, &[]) {
            Err(why) => enshwick::info!("PASS a radius of 100 m is refused: {why}"),
            Ok(_) => enshwick::warn!("FAIL a radius of 100 m was taken"),
        }
    }
}
