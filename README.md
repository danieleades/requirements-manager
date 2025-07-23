# Requiem

[![codecov](https://codecov.io/gh/danieleades/requirements/graph/badge.svg?token=xZLcLKU4D8)](https://codecov.io/gh/danieleades/requirements)
[![Continuous integration](https://github.com/danieleades/requirements/actions/workflows/CI.yml/badge.svg)](https://github.com/danieleades/requirements/actions/workflows/CI.yml)

Requiem is a plain-text requirements management tool. It is a spiritual successor to [Doorstop](https://github.com/doorstop-dev/doorstop), but aims to-

- be much, much faster
- support multiple parents per requirement
- integrate with existing plain-text documentation tools such as [Sphinx](https://github.com/sphinx-doc/sphinx) and [MdBook](https://github.com/rust-lang/mdBook).

This project is in its early stages, and is not yet ready for production use. It is currently being developed as a personal project, but contributions are welcome.

A note on naming:

The name of the package is `requirements-manager`, but the name of this project is `Requiem` (a contraction). The tool is invoked on the command line as `req`.

## Features

this is a work in progress, and will be updated as features are implemented

- [x] Manage requirements, specifications, and other documents in plain text.
- [ ] Link documents together to form a directed acyclic graph (DAG).
- [ ] Detect cycles in the graph and report them.
- [ ] Trigger reviews when dependent requirements are changed.
- [ ] Generate coverage reports
- [ ] Import and export requirements in standard formats

## Installation

```sh
cargo install requirements-manager
```

## Cli

The most up-to-date documentation for the command line interface can be found by running:

```sh
req --help
```

Quick start:

```sh
# Create a new requirements directory
mkdir my-requirements && cd my-requirements

# add a couple of user requirements
req add USR  # adds requirement USR-001
req add USR  # adds requirement USR-002

# add a system requirement
req add SYS  # adds requirement SYS-001

# link SYS-001 to USR-001
req link SYS-001 USR-001

# add a system requirement that depends on multiple user requirements
req add SYS --parents USR-001,USR-002  # adds requirement SYS-002, with links to USR-001 and USR-002
```

---

*Was this useful? [Buy me a coffee](https://github.com/sponsors/danieleades/sponsorships?sponsor=danieleades&preview=true&frequency=recurring&amount=5)*
