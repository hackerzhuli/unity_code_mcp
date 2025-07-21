# Unity Project
Here is a typical Unity project structure.

```
Assets/
ProjectSettings/
|-- ProjectVersion.txt
Library/   # for most Unity generate files
Packages/
```

The core is that `ProjectVersion.txt` is used to store the Unity version of the project(the format is YAML).

Example:
```
m_EditorVersion: 6000.0.51f1
m_EditorVersionWithRevision: 6000.0.51f1 (01c3ff5872c5)
```

just grab the `6000.0.51f1` part and use it as the version Unity.

## Unity Editor Version Detection

The `UnityProjectManager.getUnityEditorVersion()` method reads and parses the Unity editor version from the `ProjectVersion.txt` file.

**Example usage:**
```typescript
const manager = new UnityProjectManager();
await manager.init();
const version = manager.getUnityEditorVersion();
// Returns: "6000.0.51f1" or "2023.3.15f1" or null
```

**Common version string formats:**
- `"6000.0.51f1"` - Unity 6000.0.51f1
- `"2023.3.15f1"` - Unity 2023.3.15f1
- `"2022.3.42f1"` - Unity 2022.3.42f1
- `"2021.3.35f1"` - Unity 2021.3.35f1

## Find Unity Editor Process ID

The most simple way to find the running Unity Editor Process ID is to read `Library/EditorInstance.json` in the unity project, eg.

``` json
{
	"process_id" : 17328,
	"version" : "6000.0.51f1",
	"app_path" : "C:/Unity/6000.0.51f1/Editor/Unity.exe",
	"app_contents_path" : "C:/Unity/6000.0.51f1/Editor/Data"
}
```

Note that we still have to check that the actually running process(if exists)'s name is `Unity.exe`(for windows, for other OS, we need a different name).

