//! Generic transform trait for pipeline composition.
//!
//! # Example
//!
//! ```rust
//! use tola_core::Transform;
//!
//! struct AddOne;
//!
//! impl Transform<i32, i32> for AddOne {
//!     fn transform(self, input: i32) -> i32 {
//!         input + 1
//!     }
//! }
//!
//! let result = AddOne.transform(5);
//! assert_eq!(result, 6);
//! ```

/// A transformation from input type `I` to output type `O`.
///
/// This is the fundamental building block for pipeline composition.
/// Transforms are designed to be:
/// - **Stateless**: The transform consumes `self` to discourage state accumulation
/// - **Composable**: Multiple transforms can be chained together
/// - **Type-safe**: Input/output types are checked at compile time
pub trait Transform<I, O> {
    /// Apply the transformation to the input.
    fn transform(self, input: I) -> O;
}

/// Extension trait for pipeline composition.
///
/// Allows chaining transforms with `.pipe()` syntax:
///
/// ```rust
/// use tola_core::{Transform, Pipeable};
///
/// struct Double;
/// impl Transform<i32, i32> for Double {
///     fn transform(self, input: i32) -> i32 { input * 2 }
/// }
///
/// let result = 5.pipe(Double);
/// assert_eq!(result, 10);
/// ```
pub trait Pipeable: Sized {
    /// Apply a transform to this value.
    fn pipe<T, O>(self, transform: T) -> O
    where
        T: Transform<Self, O>,
    {
        transform.transform(self)
    }
}

// Implement Pipeable for all types
impl<T> Pipeable for T {}

#[cfg(test)]
mod tests {
    use super::*;

    struct AddOne;
    impl Transform<i32, i32> for AddOne {
        fn transform(self, input: i32) -> i32 {
            input + 1
        }
    }

    struct Double;
    impl Transform<i32, i32> for Double {
        fn transform(self, input: i32) -> i32 {
            input * 2
        }
    }

    #[test]
    fn test_single_transform() {
        let result = AddOne.transform(5);
        assert_eq!(result, 6);
    }

    #[test]
    fn test_pipe_syntax() {
        let result = 5.pipe(AddOne).pipe(Double);
        assert_eq!(result, 12); // (5 + 1) * 2
    }
}
