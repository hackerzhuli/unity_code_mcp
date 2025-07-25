# Changelog

All notable changes to Unity Code MCP Server will be documented in this file.

## [1.1.2] - 2025-07-25

### Improved
- Improved run tests to allow `run_tests` filter with an partial name, e.g. just class name without namespace, or just method name, still will find tests to run, but it may run more tests. This is because sometimes e.g. AI agents didn't see the namespace of the test class, so it will try to run tests with the class name.
- Better `run_tests` tool call result, when no tests are run, now it shows 0 passed, instead of 1 passed. Also, now it shows 50 passed test names, instead of just a passed test count. It helps the AI agents to confirm the expected tests were run, also saves tokens when there are too many tests.

## [1.1.1] - 2025-07-25

### Improved
- Better compile error reporting in a asset_database_refresh call when compilation didn't occur(because scripts didn't change), we report previous compile errors. This allows AI agents to see compile errors even if the actual compilation happened out of a tool call, e.g. user might have manually triggered an asset database refresh, or the compilation happened before the MCP server started

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
