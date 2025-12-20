<p align="center">
  <img src="https://vibetap.dev/logo.svg" alt="VibeTap" width="120" />
</p>

<h1 align="center">VibeTap CLI</h1>

<p align="center">
  <strong>AI-powered test generation for vibe coders</strong>
</p>

<p align="center">
  Ship fast. Ship safe. Let AI write your tests.
</p>

<p align="center">
  <a href="https://github.com/devtunehq/vibetap-cli/releases"><img src="https://img.shields.io/github/v/release/devtunehq/vibetap-cli?style=flat-square&color=blue" alt="Release" /></a>
  <a href="https://github.com/devtunehq/vibetap-cli/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="License" /></a>
  <a href="https://github.com/devtunehq/vibetap-cli/actions"><img src="https://img.shields.io/github/actions/workflow/status/devtunehq/vibetap-cli/ci.yml?style=flat-square" alt="CI" /></a>
  <a href="https://discord.gg/vibetap"><img src="https://img.shields.io/discord/1234567890?style=flat-square&label=discord&color=5865F2" alt="Discord" /></a>
</p>

<p align="center">
  <a href="#installation">Installation</a> •
  <a href="#quick-start">Quick Start</a> •
  <a href="#commands">Commands</a> •
  <a href="#features">Features</a> •
  <a href="#contributing">Contributing</a>
</p>

---

## Why VibeTap?

You're a **vibe coder**. You ship fast, iterate quickly, and don't want to slow down writing tests. But you also don't want to ship bugs.

**VibeTap is your safety net.**

```
$ vibetap now

🔍 Analyzing staged changes...

📝 3 test suggestions for src/auth/login.ts

┌─────────────────────────────────────────────────────────────────────┐
│ 1. Unit Test: validatePassword function                     [HIGH] │
├─────────────────────────────────────────────────────────────────────┤
│ Tests password validation logic including:                          │
│ • Minimum length requirement                                        │
│ • Special character validation                                      │
│ • Common password rejection                                         │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│ 2. Security Test: SQL injection prevention                  [HIGH] │
├─────────────────────────────────────────────────────────────────────┤
│ Tests that user input is properly sanitized before database queries │
└─────────────────────────────────────────────────────────────────────┘

Run 'vibetap apply 1' to add a test, or 'vibetap apply all' for everything.
```

## Installation

### One-liner (Recommended)

```bash
curl -sSL https://vibetap.dev/install.sh | sh
```

### From Releases

Download the latest binary for your platform from [GitHub Releases](https://github.com/devtunehq/vibetap-cli/releases).

| Platform | Architecture | Download |
|----------|--------------|----------|
| macOS | Apple Silicon (M1/M2/M3) | [vibetap-darwin-arm64](https://github.com/devtunehq/vibetap-cli/releases/latest) |
| macOS | Intel | [vibetap-darwin-x86_64](https://github.com/devtunehq/vibetap-cli/releases/latest) |
| Linux | x86_64 | [vibetap-linux-x86_64](https://github.com/devtunehq/vibetap-cli/releases/latest) |
| Linux | ARM64 | [vibetap-linux-arm64](https://github.com/devtunehq/vibetap-cli/releases/latest) |
| Windows | x86_64 | [vibetap-windows-x86_64.exe](https://github.com/devtunehq/vibetap-cli/releases/latest) |

### From Source

```bash
cargo install vibetap-cli
```

Or build from source:

```bash
git clone https://github.com/devtunehq/vibetap-cli.git
cd vibetap-cli
cargo build --release
```

## Quick Start

```bash
# 1. Authenticate (free tier available)
vibetap auth login

# 2. Stage some code changes
git add src/my-feature.ts

# 3. Get test suggestions
vibetap now

# 4. Apply the ones you like
vibetap apply 1
```

That's it! VibeTap analyzes your changes and suggests tests that match your project's style.

## Commands

| Command | Description |
|---------|-------------|
| `vibetap now` | Generate test suggestions for staged changes |
| `vibetap now --staged` | Alias for `vibetap now` |
| `vibetap now --quiet` | Condensed output (for git hooks) |
| `vibetap watch` | Continuous mode - suggests tests as you code |
| `vibetap apply <id>` | Apply a test suggestion to your project |
| `vibetap apply all` | Apply all suggestions |
| `vibetap revert` | Undo the last applied test |
| `vibetap run` | Run generated tests |
| `vibetap hush <file>` | Temporarily suppress suggestions for a file |
| `vibetap auth login` | Authenticate with VibeTap |
| `vibetap auth logout` | Log out |
| `vibetap auth status` | Check authentication status |
| `vibetap hook install` | Install pre-commit hook |
| `vibetap hook uninstall` | Remove pre-commit hook |

### Pre-commit Hook

Never forget to add tests before committing:

```bash
vibetap hook install
```

Now every commit will show you test suggestions:

```
$ git commit -m "Add user auth"

🧪 VibeTap: 2 test suggestions for staged changes
   1. Unit test for login flow
   2. Security test for password validation

Run 'vibetap apply' to add tests, or '--no-verify' to skip.
```

## Features

### Smart Test Generation

VibeTap doesn't just generate generic tests. It:

- **Learns your style** - Matches your existing test patterns
- **Understands context** - Knows what functions do and how they're used
- **Prioritizes risk** - Focuses on security and edge cases first
- **Supports frameworks** - Vitest, Jest, Pytest, Go testing, and more

### Security First

Every scan includes security checks:

- OWASP Top 10 vulnerabilities
- Injection attacks (SQL, XSS, command)
- Authentication bypasses
- Sensitive data exposure
- Input validation gaps

### Framework Support

| Language | Frameworks |
|----------|------------|
| TypeScript/JavaScript | Vitest, Jest, Mocha, Playwright |
| Python | Pytest, unittest |
| Go | testing, testify |
| Rust | cargo test |
| Ruby | RSpec, Minitest |

## Configuration

### Project Config

Create `.vibetap/config.json` in your project:

```json
{
  "version": "1.0",
  "testRunner": "vitest",
  "testDirectory": "tests",
  "generation": {
    "maxSuggestions": 5,
    "includeSecurity": true,
    "includeNegativePaths": true
  },
  "ignore": [
    "*.config.ts",
    "migrations/**"
  ]
}
```

### Global Config

Located at `~/.config/vibetap/config.toml`:

```toml
[api]
key = "vt_..."

[generation]
default_runner = "vitest"
```

## How It Works

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   Your Code     │────▶│   VibeTap CLI   │────▶│   VibeTap API   │
│  (git staged)   │     │  (local agent)  │     │   (AI engine)   │
└─────────────────┘     └─────────────────┘     └─────────────────┘
                                                        │
                                                        ▼
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  Applied Tests  │◀────│ Your Approval   │◀────│ Test Suggestions│
│   (in repo)     │     │  (vibetap apply)│     │ (prioritized)   │
└─────────────────┘     └─────────────────┘     └─────────────────┘
```

1. **Analyze** - VibeTap reads your staged git changes
2. **Identify** - AI identifies what needs testing
3. **Generate** - Tests are created matching your style
4. **Review** - You see suggestions with explanations
5. **Apply** - One command adds tests to your project

## Privacy

- Only git diffs are sent (not full files)
- No code is stored on our servers
- All API calls use TLS encryption
- See our [Privacy Policy](https://vibetap.dev/privacy)

## Contributing

We love contributions! VibeTap is open source under the MIT license.

### Development Setup

```bash
# Clone the repo
git clone https://github.com/devtunehq/vibetap-cli.git
cd vibetap-cli

# Build
cargo build

# Run tests
cargo test

# Run the CLI
cargo run -- now
```

### Project Structure

```
vibetap-cli/
├── crates/
│   ├── vibetap-cli/     # CLI binary
│   ├── vibetap-core/    # Core logic
│   └── vibetap-git/     # Git integration
├── scripts/
│   └── install.sh       # Cross-platform installer
└── Cargo.toml           # Workspace config
```

### Submitting PRs

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing`)
5. Open a Pull Request

## Support

- **Documentation**: [vibetap.dev/docs](https://vibetap.dev/docs)
- **Discord**: [discord.gg/vibetap](https://discord.gg/vibetap)
- **Issues**: [GitHub Issues](https://github.com/devtunehq/vibetap-cli/issues)
- **Twitter**: [@vibetapdev](https://twitter.com/vibetapdev)

## License

MIT License - see [LICENSE](LICENSE) for details.

---

<p align="center">
  Made with ❤️ by <a href="https://devtunehq.com">DevTune HQ</a> for vibe coders everywhere.
</p>
