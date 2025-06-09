# Contributing to Reflow

Thank you for your interest in contributing to Reflow! This document provides guidelines and information for contributors.

## Getting Started

### Prerequisites

Before contributing, ensure you have:

- **Rust** (latest stable version)
- **Node.js** (version 18 or higher)
- **Git** for version control
- **mdBook** for documentation (optional)

### Setting Up the Development Environment

1. **Clone the repository:**
   ```bash
   git clone https://github.com/your-org/reflow.git
   cd reflow
   ```

2. **Install Rust dependencies:**
   ```bash
   cargo build
   ```

3. **Install Node.js dependencies:**
   ```bash
   cd examples/audio-flow
   npm install
   ```

4. **Run tests:**
   ```bash
   cargo test
   ```

## How to Contribute

### Reporting Issues

- Use the [GitHub Issues](https://github.com/your-org/reflow/issues) page
- Provide detailed information about the bug or feature request
- Include relevant code examples and error messages
- Search existing issues before creating new ones

### Submitting Pull Requests

1. **Fork the repository** and create a new branch
2. **Make your changes** with clear, descriptive commits
3. **Add tests** for new functionality
4. **Update documentation** as needed
5. **Submit a pull request** with a clear description

### Code Style Guidelines

#### Rust Code
- Follow the [Rust Style Guide](https://doc.rust-lang.org/nightly/style-guide/)
- Use `cargo fmt` to format code
- Use `cargo clippy` to catch common mistakes
- Write comprehensive documentation comments (`///`)

#### JavaScript/TypeScript Code
- Use Prettier for formatting
- Follow ESLint recommendations
- Use meaningful variable and function names
- Write JSDoc comments for public APIs

#### Documentation
- Use clear, concise language
- Include code examples where appropriate
- Test all code examples to ensure they work
- Follow the existing documentation structure

## Development Workflow

### Branch Naming Convention

- `feature/description` - for new features
- `fix/description` - for bug fixes
- `docs/description` - for documentation updates
- `refactor/description` - for code refactoring

### Commit Message Format

```
type(scope): brief description

Detailed explanation of the change, if necessary.

Fixes #123
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

### Testing

#### Unit Tests
```bash
cargo test
```

#### Integration Tests
```bash
cargo test --test integration
```

#### Documentation Tests
```bash
cargo test --doc
```

#### End-to-End Tests
```bash
cd examples/audio-flow
npm test
```

## Documentation Guidelines

### Writing Style
- Use active voice
- Write in present tense
- Be clear and concise
- Include practical examples
- Explain the "why" not just the "how"

### Code Examples
- Test all code examples
- Include imports and setup code
- Show expected output where relevant
- Use realistic, meaningful examples

### API Documentation
- Document all public functions and types
- Include parameter descriptions
- Provide return value information
- Add usage examples

## Community Guidelines

### Code of Conduct

We are committed to providing a welcoming and inclusive environment for all contributors. Please:

- Be respectful and considerate
- Focus on constructive feedback
- Help others learn and grow
- Celebrate diverse perspectives

### Communication Channels

- **GitHub Issues**: Bug reports and feature requests
- **GitHub Discussions**: General questions and community discussion
- **Discord**: Real-time chat (invite link in README)

## Release Process

### Versioning

We follow [Semantic Versioning](https://semver.org/):
- `MAJOR.MINOR.PATCH`
- Breaking changes increment MAJOR
- New features increment MINOR
- Bug fixes increment PATCH

### Release Checklist

1. Update version numbers in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Run full test suite
4. Update documentation
5. Create GitHub release
6. Publish to crates.io (maintainers only)

## Architecture Guidelines

### Actor Design Principles

When creating new actors:

- **Single Responsibility**: Each actor should have one clear purpose
- **Immutable Messages**: Messages should be immutable data structures
- **Error Handling**: Handle errors gracefully and provide meaningful messages
- **Documentation**: Include comprehensive examples and usage guidelines

### Component Design

For new components:

- **Composability**: Components should work well with others
- **Configuration**: Use clear, typed configuration options
- **Performance**: Consider memory usage and execution speed
- **Testing**: Include unit tests and integration tests

### API Design

When designing APIs:

- **Consistency**: Follow existing patterns and conventions
- **Type Safety**: Use strong typing where possible
- **Documentation**: Provide clear documentation and examples
- **Backwards Compatibility**: Consider impact on existing users

## Getting Help

If you need help with contributing:

1. Check the [documentation](../README.md)
2. Search [existing issues](https://github.com/your-org/reflow/issues)
3. Ask in [GitHub Discussions](https://github.com/your-org/reflow/discussions)
4. Join our [Discord community](discord-invite-link)

## Recognition

Contributors are recognized in:
- The project README
- Release notes
- Annual contributor reports

We appreciate all contributions, whether they're code, documentation, testing, or community support!

## License

By contributing to Reflow, you agree that your contributions will be licensed under the same license as the project (see [LICENSE](../../LICENSE) file).
