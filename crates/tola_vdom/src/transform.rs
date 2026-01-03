//! Transform system for VDOM processing pipeline.
//!
//! This module provides utilities for composing document transformations
//! in a type-safe, pipeline-oriented manner.

use crate::phase::Phase;

/// Document transformation trait.
///
/// Transforms convert documents from one phase to another or
/// modify documents within the same phase.
pub trait VdomTransform<P: Phase> {
    /// Output phase after transformation.
    type Output;

    /// Perform the transformation.
    fn transform(self, input: P) -> Self::Output;
}

/// Pipeline builder for composing transforms.
pub struct Pipeline<T> {
    value: T,
}

impl<T> Pipeline<T> {
    /// Create a new pipeline with initial value.
    pub fn new(value: T) -> Self {
        Self { value }
    }

    /// Apply a transform to the pipeline.
    pub fn pipe<F, U>(self, transform: F) -> Pipeline<U>
    where
        F: FnOnce(T) -> U,
    {
        Pipeline {
            value: transform(self.value),
        }
    }

    /// Extract the final value from the pipeline.
    pub fn finish(self) -> T {
        self.value
    }
}

/// Extension trait for piping values through transformations.
pub trait Pipeable: Sized {
    /// Pipe this value through a transformation.
    fn pipe<F, U>(self, f: F) -> U
    where
        F: FnOnce(Self) -> U,
    {
        f(self)
    }
}

// Implement for all types
impl<T> Pipeable for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline() {
        let result = Pipeline::new(1)
            .pipe(|x| x + 1)
            .pipe(|x| x * 2)
            .finish();

        assert_eq!(result, 4);
    }

    #[test]
    fn test_pipeable() {
        let result = 1.pipe(|x| x + 1).pipe(|x| x * 2);
        assert_eq!(result, 4);
    }
}
