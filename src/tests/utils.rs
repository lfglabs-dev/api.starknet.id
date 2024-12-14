use crate::utils::{clean_string, extract_prefix_and_root, to_u256};
use ark_ff::{biginteger::BigInteger256, BigInteger};

#[cfg(test)]
mod extract_prefix_and_root {
    use super::*;

    #[test]
    fn test_standard_domain() {
        let (prefix, root) = extract_prefix_and_root("sub.example.com".to_string());
        assert_eq!(prefix, "sub.");
        assert_eq!(root, "example.com");
    }

    #[test]
    fn test_multiple_subdomains() {
        let (prefix, root) = extract_prefix_and_root("deep.nested.sub.example.com".to_string());
        assert_eq!(prefix, "deep.nested.sub.");
        assert_eq!(root, "example.com");
    }

    #[test]
    fn test_no_subdomain() {
        let (prefix, root) = extract_prefix_and_root("example.com".to_string());
        assert_eq!(prefix, "");
        assert_eq!(root, "example.com");
    }

    #[test]
    fn test_single_part() {
        let (prefix, root) = extract_prefix_and_root("localhost".to_string());
        assert_eq!(prefix, "");
        assert_eq!(root, "localhost");
    }

    #[test]
    fn test_empty_string() {
        let (prefix, root) = extract_prefix_and_root("".to_string());
        assert_eq!(prefix, "");
        assert_eq!(root, "");
    }

    #[test]
    fn test_with_trailing_dot() {
        let (prefix, root) = extract_prefix_and_root("sub.example.com.".to_string());
        assert_eq!(prefix, "sub.example.");
        assert_eq!(root, "com.");
    }

    #[test]
    fn test_complex_tld() {
        let (prefix, root) = extract_prefix_and_root("service.example.co.uk".to_string());
        assert_eq!(prefix, "service.example.");
        assert_eq!(root, "co.uk");
    }

    #[test]
    fn test_dots_only() {
        let (prefix, root) = extract_prefix_and_root("...".to_string());
        assert_eq!(prefix, "..");
        assert_eq!(root, ".");
    }

    #[test]
    fn test_unicode_domain() {
        let (prefix, root) = extract_prefix_and_root("sub.例子.com".to_string());
        assert_eq!(prefix, "sub.");
        assert_eq!(root, "例子.com");
    }
}

#[cfg(test)]
mod to_u256 {
    use super::*;

    #[test]
    fn test_to_u256_valid_inputs() {
        let low = "0x00000000000000000000000000000001";
        let high = "0x00000000000000000000000000000000";

        let result = to_u256(low, high);

        // Check if the result is within the valid range
        let min_value = BigInteger256::from_bits_be(&[false; 256][..]);
        let max_value = BigInteger256::from_bits_be(&[true; 256][..]);

        assert!(result >= min_value);
        assert!(result <= max_value);
    }

    #[test]
    fn test_to_u256_invalid_inputs() {
        let low = "invalid hex";
        let high = "0x00000000000000000000000000000000";

        let result = std::panic::catch_unwind(||to_u256(low, high));

        assert!(result.is_err());
    }

    #[test]
    fn test_to_u256_edge_cases() {
        let low = "0x0000000000000000";
        let high = "0x0000000000000001";

        let result = to_u256(low, high);

        assert_eq!(result, BigInteger256::from_bits_be(&[false; 32][..]));
    }

    #[test]
    fn test_to_u256_zero_value() {
        let low = "0x0000000000000000";
        let high = "0x0000000000000000";

        let result = to_u256(low, high);

        assert_eq!(result, BigInteger256::from_bits_be(&[false; 32][..]));
    }
}

#[cfg(test)]
mod clean_string {
    use super::*;

    #[test]
    fn test_clean_string_no_nulls() {
        let input = "Hello, world!";
        let result = clean_string(input);
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn test_clean_string_with_nulls() {
        let input = "Hello\0, world\0!";
        let result = clean_string(input);
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn test_clean_string_only_nulls() {
        let input = "\0\0\0";
        let result = clean_string(input);
        assert_eq!(result, "");
    }

    #[test]
    fn test_clean_string_empty_string() {
        let input = "";
        let result = clean_string(input);
        assert_eq!(result, "");
    }

    #[test]
    fn test_clean_string_unicode_chars() {
        let input = "Hell\0o 🌍\0!";
        let result = clean_string(input);
        assert_eq!(result, "Hello 🌍!");
    }
}
