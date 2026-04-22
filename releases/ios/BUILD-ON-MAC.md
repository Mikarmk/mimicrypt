iOS build handoff for Mimicrypt

What is included:

- `Mimicrypt-iOS-project.zip`: generated Tauri iOS Xcode project

What is required on the build machine:

- full Xcode app installed and selected with `xcode-select`
- Apple Developer signing certificate
- provisioning profile / development team
- CocoaPods
- Rust toolchain
- Node.js / npm

Recommended build machine flow:

```bash
cd apps/desktop-tauri
npm install
npx tauri ios init
npx tauri ios build --export-method debugging
```

Current local blocker on this machine:

- only Command Line Tools are installed instead of full Xcode
- `xcrun simctl list runtimes --json` fails
- `0 valid identities found` for Apple code signing

This means a real installable `.ipa` cannot be produced on the current machine.
