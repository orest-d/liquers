use serde::{Serialize, Deserialize};

/// Type-safe handle for UI elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UIHandle(pub u64);

impl From<u64> for UIHandle {
    fn from(id: u64) -> Self {
        UIHandle(id)
    }
}

impl From<UIHandle> for u64 {
    fn from(handle: UIHandle) -> Self {
        handle.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_u64() {
        let h: UIHandle = 42u64.into();
        assert_eq!(h, UIHandle(42));
    }

    #[test]
    fn test_into_u64() {
        let n: u64 = UIHandle(42).into();
        assert_eq!(n, 42);
    }

    #[test]
    fn test_roundtrip() {
        let original: u64 = 123;
        let handle: UIHandle = original.into();
        let back: u64 = handle.into();
        assert_eq!(original, back);
    }
}
