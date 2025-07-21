# Project
## Why
The goal of this project is to create reliable MCP tools that AI can rely on to communicate with Unity Editor. The problem with some of the other Unity MCP server is their inability to deal with domain reload, that will make Unity Editor side components unavailable.

So we have to build a new MCP server that is more reliable. The features we have is the basics, just get Unity logs and get and run unity tests. That's it.

Also this project with be a component of our Unity Code Pro extension for VS Code. So users can set up VS Code (or popular forks) to use our server.

Tech: we are going to use rust to do our MCP server.

The core crate is `rust-mcp-sdk`, and we use tokio for async IO(which is implicitly installed because of `rust-mcp-sdk` depends on it).

We're going to do single threaded async to avoid complexity.


