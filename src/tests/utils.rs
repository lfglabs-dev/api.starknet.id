use crate::utils::extract_prefix_and_root;

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
