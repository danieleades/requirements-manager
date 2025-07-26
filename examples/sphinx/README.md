# Sphinx

Requiem can be used alongside MdBook. The only requirement is that files which are not requirements do not use filenames which have the same syntax as a Requiem human readable ID (HRID).

## Building the Example

requires the [UV package manager](https://docs.astral.sh/uv/getting-started/installation/).

then build the book

```sh
uv run make html
```

## File Format

Since Requiem uses Markdown files, you will need to configure Sphinx to support Markdown as well as the default reStructuredText. You can do this by adding the [Myst Parser](https://myst-parser.readthedocs.io/en/latest/) plugin, as per this example.
