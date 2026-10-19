//! What the tree knows about the world around it.
//!
//! The game or viewer implements [`Habitat`] against its own scene (ground,
//! rocks, buildings, other trees) and passes it to [`Tree::grow`], which
//! refuses to put a new internode where the world is occupied. The trait is
//! engine-neutral so trees-core never depends on the host's scene types.
//!
//! [`Tree::grow`]: crate::Tree::grow

use glam::Vec3;

/// The world a tree grows in, as seen by the growth logic.
pub trait Habitat {
    /// Whether `point` (in the tree's world space) is filled by something a
    /// branch cannot grow into, such as the ground or a rock.
    fn occupied(&self, point: Vec3) -> bool;
}

/// Empty space: nothing is ever occupied. Useful for tests and for trees
/// grown with no surroundings.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenAir;

impl Habitat for OpenAir {
    fn occupied(&self, _point: Vec3) -> bool {
        false
    }
}

/// Any `Fn(Vec3) -> bool` is a habitat, so a quick test or one-off scene can
/// pass a closure.
impl<F: Fn(Vec3) -> bool> Habitat for F {
    fn occupied(&self, point: Vec3) -> bool {
        self(point)
    }
}
