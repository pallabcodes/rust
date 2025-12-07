pub mod greeting {
    pub fn greet(name: &str) -> String {
        format!("Hello, {name}! Welcome to the Rust monorepo.")
    }
}

pub mod math {
    use thiserror::Error;

    #[derive(Debug, Error, PartialEq)]
    pub enum MathError {
        #[error("overflow when adding {left} + {right}")]
        Overflow { left: i64, right: i64 },
    }

    pub fn checked_add(left: i64, right: i64) -> Result<i64, MathError> {
        left.checked_add(right)
            .ok_or(MathError::Overflow { left, right })
    }
}

#[cfg(test)]
mod tests {
    use super::greeting;
    use super::math;

    #[test]
    fn greeting_is_stable() {
        let text = greeting::greet("Tess");
        assert!(text.contains("Tess"));
    }

    #[test]
    fn checked_add_handles_overflow() {
        let res = math::checked_add(i64::MAX, 1);
        assert!(res.is_err());
    }
}

