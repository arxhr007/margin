# Code signing

Windows releases are signed through [SignPath](https://signpath.io), which signs open-source projects
for free with a certificate issued to SignPath Foundation. A signed `margin.exe` builds download
reputation with Microsoft SmartScreen, Edge and Chrome under one certificate instead of starting from
zero with every release, so the "not commonly downloaded" warnings fade over time.

The release workflow ([build.yml](../.github/workflows/build.yml)) signs automatically once SignPath
is set up. Until then it builds unsigned releases exactly as before.

## One-time setup

1. **Apply** for the SignPath Foundation open-source program at <https://signpath.org> with this
   repository. The README's [Code signing policy](../README.md#code-signing-policy) section is the
   policy page they ask for.
2. **After approval**, in the SignPath web app:
   - Create (or use the provided) project with the slug `margin`. If you pick another slug, set the
     repository variable `SIGNPATH_PROJECT_SLUG` to it.
   - Link the project to this GitHub repository as a trusted build system, as SignPath's GitHub
     guide describes.
   - Set the project's artifact configuration to the contents of
     [.signpath/artifact-configuration.xml](../.signpath/artifact-configuration.xml).
   - Check that the signing policies are named `release-signing` and `test-signing`.
   - Create an API token for a CI user that may submit signing requests.
3. **In this repository** (Settings → Secrets and variables → Actions):
   - Secret `SIGNPATH_API_TOKEN`: the API token.
   - Variable `SIGNPATH_ORGANIZATION_ID`: your SignPath organization ID.
4. **Test it**: run the workflow by hand from the Actions tab (it uses `test-signing`), then push a
   `v*` tag for a real release. Release signing waits up to an hour for an approver to confirm the
   request in SignPath.

## Checking a release

```powershell
Get-AuthenticodeSignature .\margin.exe | Format-List Status, SignerCertificate
```

`Status` should be `Valid`, with SignPath Foundation as the signer.
