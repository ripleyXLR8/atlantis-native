# Signing the Windows build

[Back to the README](../README.md)

The workflow signs `atlantis-native.exe` through
[SignPath](https://about.signpath.io/) once the repository has:

| Setting (Settings > Secrets and variables > Actions) | Kind | Value |
|---|---|---|
| `SIGNPATH_API_TOKEN` | secret | an API token of a SignPath CI user with submitter rights |
| `SIGNPATH_ORGANIZATION_ID` | variable | the SignPath organisation's id |
| `SIGNPATH_PROJECT_SLUG` | variable | the project's slug, e.g. `atlantis-native` |
| `SIGNPATH_SIGNING_POLICY_SLUG` | variable | `release-signing` (or `test-signing` first) |

Without the secret the build stays unsigned, as before.

The project's artifact configuration in SignPath (the GitHub artifact is
a zip holding the executable):

```xml
<?xml version="1.0" encoding="utf-8"?>
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <zip-file>
    <pe-file path="atlantis-native.exe">
      <authenticode-sign/>
    </pe-file>
  </zip-file>
</artifact-configuration>
```

The SignPath GitHub App must be installed on the repository, and the
trusted build system "GitHub.com" linked to the project, so that SignPath
accepts only artifacts built by this repository's workflow.
