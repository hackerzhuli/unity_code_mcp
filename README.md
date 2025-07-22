# Unity Code MCP
## Test
When running tests, you need to have a running Unity Editor instance, opening the embedded Unity project in `UnityProject`.

Also you must run single threaded, ie. `cargo test -- --test-threads=1`.

## Build
aws-lc-rs requires cmake. [guide for windows](https://aws.github.io/aws-lc-rs/requirements/windows.html)

For windows install cmake and use your compiler to compile aws-lc-rs(cargo will run it).

