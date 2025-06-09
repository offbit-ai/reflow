# Reflow Documentation - mdBook Setup

This directory contains the complete mdBook setup for the Reflow documentation, providing a beautiful, searchable, and navigable online documentation experience.

## 📚 What's Included

- **Complete mdBook Configuration** (`book.toml`)
- **Structured Navigation** (`docs/SUMMARY.md`)
- **Custom Theme** with Reflow branding
- **Enhanced Features** including code copying, keyboard shortcuts, and responsive design
- **All Documentation** organized into logical sections

## 🚀 Quick Start

### Prerequisites

Install mdBook and useful plugins:

```bash
# Install mdBook
cargo install mdbook

# Install useful plugins (optional but recommended)
cargo install mdbook-toc
cargo install mdbook-admonish
cargo install mdbook-mermaid
```

### Building the Documentation

```bash
# Build the book
mdbook build

# Serve locally with live reload
mdbook serve

# Serve on a specific port
mdbook serve --port 8080

# Build and open in browser
mdbook serve --open
```

The documentation will be available at `http://localhost:3000` by default.

## 📖 Book Structure

```
📁 Reflow Documentation
├── 🏠 Introduction
├── 🚀 Getting Started
│   ├── Installation
│   ├── Basic Concepts
│   ├── Development Setup
│   └── Your First Workflow
├── 🏗️ Architecture
│   ├── Overview
│   ├── Actor Model
│   ├── Message Passing
│   └── Graph System
├── 📋 API Documentation
│   ├── Working with Actors
│   └── Graph API
│       ├── Creation
│       ├── Analysis
│       ├── Layout
│       └── Advanced Features
├── 🧩 Components & Scripting
│   ├── Standard Library
│   └── JavaScript & Deno Runtime
├── 📚 Tutorials
│   ├── Building a Visual Editor
│   ├── ReactFlow Integration
│   └── Performance Optimization
├── 🚀 Deployment
│   └── Native Deployment
├── 💡 Examples
│   └── Example Projects
├── 📖 Reference
│   ├── API Reference
│   └── Troubleshooting Guide
└── 📋 Appendices
    ├── Glossary
    └── Contributing
```

## 🎨 Custom Features

### Enhanced User Experience
- **🎨 Custom Reflow Theme** with brand colors and professional styling
- **⌨️ Keyboard Shortcuts** for navigation and search
- **📋 Code Copy Buttons** on all code blocks
- **📊 Reading Progress Bar** to track documentation progress
- **🔍 Enhanced Search** with improved highlighting
- **📱 Responsive Design** for mobile and desktop

### Keyboard Shortcuts
- `Ctrl+S` - Focus search bar
- `H` - Toggle sidebar
- `T` - Toggle theme (light/dark)
- `←/→` - Navigate between pages
- `⌨️` icon (top right) - Show all shortcuts

### Code Features
- **Syntax Highlighting** for Rust, JavaScript, TypeScript, JSON, TOML, etc.
- **Copy to Clipboard** buttons on all code blocks
- **Line Numbers** for better code reference
- **Responsive Code Blocks** that work on mobile

## 🛠️ Customization

### Modifying the Theme

Edit the custom theme files:
- `docs/theme/custom.css` - Custom styling and colors
- `docs/theme/custom.js` - Interactive features and enhancements

### Adding New Sections

1. Create new markdown files in the `docs/` directory
2. Update `docs/SUMMARY.md` to include the new sections
3. Rebuild the book with `mdbook build`

### Configuration

Modify `book.toml` to:
- Change book metadata (title, authors, description)
- Configure output options
- Enable/disable preprocessors
- Customize build settings

## 📦 Deployment Options

### GitHub Pages

1. **Enable GitHub Pages** in repository settings
2. **Set source** to GitHub Actions
3. **Create workflow** (`.github/workflows/mdbook.yml`):

```yaml
name: Deploy mdBook
on:
  push:
    branches: [ main ]
    paths: [ 'docs/**' ]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v3
    - name: Setup mdBook
      uses: peaceiris/actions-mdbook@v1
      with:
        mdbook-version: 'latest'
    - run: mdbook build
    - name: Deploy
      uses: peaceiris/actions-gh-pages@v3
      with:
        github_token: ${{ secrets.GITHUB_TOKEN }}
        publish_dir: ./book
```

### Netlify

1. Connect your repository to Netlify
2. Set build command: `mdbook build`
3. Set publish directory: `book`

### Custom Domain

Update `book.toml`:
```toml
[output.html]
cname = "docs.yourdomain.com"
```

## 🔧 Development

### File Structure
```
├── book.toml              # mdBook configuration
├── docs/                  # Source documentation
│   ├── SUMMARY.md         # Table of contents
│   ├── README.md          # Introduction page
│   ├── theme/             # Custom theme assets
│   ├── getting-started/   # Getting started guides
│   ├── architecture/      # Architecture documentation
│   ├── api/              # API documentation
│   ├── tutorials/        # Tutorials and guides
│   ├── reference/        # Reference materials
│   └── appendices/       # Glossary, contributing, etc.
└── book/                 # Generated output (gitignored)
```

### Adding New Content

1. **Create markdown files** in appropriate subdirectories
2. **Update SUMMARY.md** to include new pages in navigation
3. **Use relative links** for cross-references
4. **Test locally** with `mdbook serve`

### Best Practices

- **Use descriptive headings** for better navigation
- **Include code examples** where relevant
- **Add cross-references** between related sections
- **Test all code examples** to ensure they work
- **Keep pages focused** on specific topics
- **Use consistent formatting** throughout

## 🎯 Features Comparison

| Feature | Standard mdBook | Reflow mdBook |
|---------|----------------|---------------|
| Basic Documentation | ✅ | ✅ |
| Search | ✅ | ✅ Enhanced |
| Themes | ✅ Basic | ✅ Custom Reflow |
| Code Highlighting | ✅ | ✅ |
| Code Copying | ❌ | ✅ |
| Keyboard Shortcuts | ❌ | ✅ |
| Progress Tracking | ❌ | ✅ |
| Mobile Responsive | ✅ Basic | ✅ Enhanced |
| Print Support | ✅ | ✅ |

## 🤝 Contributing

To contribute to the documentation:

1. **Edit markdown files** in the `docs/` directory
2. **Follow the style guide** in `docs/appendices/contributing.md`
3. **Test locally** with `mdbook serve`
4. **Submit a pull request** with your changes

## 📞 Support

- **Documentation Issues**: Create an issue in the repository
- **mdBook Help**: Visit [mdBook documentation](https://rust-lang.github.io/mdBook/)
- **Community**: Join our Discord for real-time help

---

**Happy documenting! 📚✨**

The Reflow documentation is now ready to provide an exceptional reading experience for your users.
