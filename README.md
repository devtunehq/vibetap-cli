# VibeTap CLI

AI-powered test generation for your codebase. VibeTap watches your staged git changes and suggests relevant tests.

## Installation

```bash
cargo install vibetap
```

Or download pre-built binaries from the [releases page](https://github.com/devtunehq/vibetap-cli/releases).

## Quick Start

```bash
# Initialize VibeTap in your project
vibetap init

# Watch for staged changes and get test suggestions
vibetap watch

# Get suggestions for current staged changes
vibetap now --staged

# Apply a suggested test
vibetap apply <suggestion-id>
```

## Commands

| Command | Description |
|---------|-------------|
| `init` | Initialize VibeTap in your project |
| `watch` | Watch for staged changes and suggest tests |
| `now` | Generate test suggestions for current changes |
| `apply` | Apply a test suggestion to your codebase |
| `revert` | Revert a previously applied test |
| `hush` | Suppress suggestions for specific files |
| `run` | Run generated tests |

## Configuration

VibeTap stores configuration in two places:

- **Global**: `~/.config/vibetap/config.toml` - API keys and global settings
- **Project**: `.aitest/config.json` - Project-specific configuration

### Authentication

VibeTap uses the VibeTap SaaS API for test generation. Sign up at [vibetap.dev](https://vibetap.dev) and get your API key.

```bash
vibetap auth login
```

### BYOK (Bring Your Own Key)

For teams that want to use their own LLM API keys, configure BYOK mode in the dashboard.

## License

MIT License - see [LICENSE](LICENSE) for details.
