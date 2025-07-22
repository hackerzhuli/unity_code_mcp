# Unity Code MCP
## Motivation
The reason I created this mcp server is because existing MCP server can't handle Unity compilation correctly. When Unity is compiling, and a tool call happens, the tool calls will just fail. Which is very bad.

I created this mcp server to enable AI agent to refresh asset database (which means compilation), also run tests reliably. Even when AI agent called the tool while Unity is in the middle of a compilation. 

## Description
This mcp allows AI agents to write code without human intervention for as long as possible. This is only focused on coding, no asset management or scene management. 

We have a simplified list to just two tools for AI, which saves tools calls and token usage as well.

1. Refresh asset database. The result will contain error logs during the refresh, which includes compilation errors.
2. Run tests. The result will contain all the tests that are run, whether they passed, and their logs and error stack trace.

We don't give AI a way to randomly look at logs because that is just a waste of tokens, only compile errors or logs and erros during tests matter to AI, which is included in the result of refresh asset database and run tests.

These two tools allow AI agent to be in a loop of writing code, compile the code, fix compile errors, run tests against them, fix bugs according to failed tests, until the relavant tests pass. Which enables max productivity from using AI agents. 

## Installtion
1. Install the Unity package [Visual Studio Code Editor](https://github.com/hackerzhuli/com.hackerzhuli.code)
2. Build or download the binary of this project. 
3. Add this manually to your code editor.

``` json
{
  "mcpServers": {
    "server-name": {
      "command": "/path/to/unity-code-mcp-server",
      "env": {
        "UNITY_PROJECT_PATH": "path/to/unity/project"
      }
    }
  }
}
```

## Test
When running tests, you need to have a running Unity Editor instance, opening the embedded Unity project in `UnityProject`.

Also you must run single threaded, ie. `cargo test -- --test-threads=1`.

## Build
aws-lc-rs requires cmake. [guide for windows](https://aws.github.io/aws-lc-rs/requirements/windows.html)

For windows install cmake and use your compiler to compile aws-lc-rs(cargo will run it).

