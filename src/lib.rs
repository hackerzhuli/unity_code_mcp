//! Unity Code MCP Server
//! 
//! A Model Context Protocol (MCP) server for Unity Editor integration.
//! This crate provides tools for communicating with Unity Editor via UDP messaging
//! and managing Unity project information.

pub mod unity_project_manager;
pub mod unity_messaging_client;

pub use unity_project_manager::UnityProjectManager;
pub use unity_messaging_client::{UnityMessagingClient, Message, MessageType, UnityMessagingError};