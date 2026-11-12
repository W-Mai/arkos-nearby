//! Input provenance shared by the interface and its owner.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Generation(u64);

impl Generation {
    pub const FIRST: Self = Self(1);

    pub fn advance(&mut self) {
        self.0 = self.0.checked_add(1).expect("Input generation exhausted");
    }

    pub fn bind<T>(self, payload: T) -> Sourced<T> {
        Sourced {
            generation: self,
            payload,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sourced<T> {
    pub generation: Generation,
    pub payload: T,
}

impl<T> Sourced<T> {
    pub fn map<U>(self, map: impl FnOnce(T) -> U) -> Sourced<U> {
        self.generation.bind(map(self.payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_effects_retain_their_origin_when_the_owner_advances() {
        let mut generation = Generation::FIRST;
        let queued = generation.bind("confirm");
        generation.advance();
        assert_ne!(queued.generation, generation);
        let mapped = queued.map(str::to_owned);
        assert_eq!(mapped.generation, Generation::FIRST);
        assert_eq!(mapped.payload, "confirm");
        assert_ne!(Generation::default(), Generation::FIRST);
    }
}
