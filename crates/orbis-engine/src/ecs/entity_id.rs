use legion::Entity;
use std::mem;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct EntityId(pub(crate) u64);

pub trait EntityRawU64 {
    fn as_raw_u64(&self) -> u64;
    fn from_raw_u64(raw: u64) -> Self;
}

impl EntityRawU64 for Entity {
    fn as_raw_u64(&self) -> u64 {
        unsafe {
            let raw64: u64 = mem::transmute(*self);
            raw64
        }
    }

    fn from_raw_u64(raw: u64) -> Self {
        unsafe {
            let raw64: u64 = raw;
            mem::transmute::<u64, Entity>(raw64)
        }
    }
}

impl From<Entity> for EntityId {
    fn from(e: Entity) -> Self {
        EntityId(e.as_raw_u64())
    }
}

impl From<EntityId> for Entity {
    fn from(id: EntityId) -> Self {
        Entity::from_raw_u64(id.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use legion::*;

    #[test]
    fn test_entity_to_raw_u64_and_back() {
        let mut world = World::default();
        let e = world.push((10.0,));

        let e_id = EntityId::from(e);
        let result: Entity = e_id.into();

        assert_eq!(e, result);
    }

    #[test]
    // Ricostruzione come nello shader: high << 32 | low
    // Da usare nello shader per ricostruire entity_id (u64) da vec2<u32>
    fn test_reconstruct_u64_from_u32() {
        // Valore u64 più grande di u32::MAX
        let original: u64 = u32::MAX as u64 + 48; // 4.294.967.343

        // Split in low/high 32 bit
        let low: u32 = original as u32; // parte bassa
        let high: u32 = (original >> 32) as u32; // parte alta

        let reconstructed: u64 = (high as u64) << 32 | (low as u64);

        // Verifica
        assert_eq!(original, reconstructed);

        // Stampa per controllo
        println!("original = {}", original);
        println!("low = {}, high = {}", low, high);
        println!("reconstructed = {}", reconstructed);
    }
}
