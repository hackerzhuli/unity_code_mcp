# Changelog

All notable changes to Unity Code MCP Server will be documented in this file.

## [1.1.0] - 2025-07-24

### Added
- Play mode state tracking to prevent actions during play mode
- Test run tracking with conflict prevention
- Reduce logs memory usage with size limits and smart log deduplication

### Fixed
- Correct test result counts for failed and skipped tests
- Graceful handling of unexpected Unity Editor shutdown

## [1.0.0] - 2025-7-23

### Added
- Initial release of Unity Code MCP Server
- Asset database refresh tool with compilation error reporting
- Test execution tool with comprehensive test results and logs
- Compilation-resilient architecture that survives Unity domain reloads
- Self-contained Rust binary with no runtime dependencies
- Support for Unity 6.0 and higher
- Cross-platform compatibility (Windows, macOS, Linux)

### Features
- **Coding-focused design**: Only essential tools for AI-driven code development
- **High performance**: Built in Rust for speed and minimal resource usage
- **Robust error handling**: Graceful handling of Unity compilation cycles
- **Comprehensive test support**: Both EditMode and PlayMode test execution
