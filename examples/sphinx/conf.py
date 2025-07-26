project = 'Sphinx Example'
copyright = '2025, daniel eades'
author = 'daniel eades'

extensions = ["myst_parser"]

templates_path = ['_templates']
exclude_patterns = ['_build', 'Thumbs.db', '.DS_Store', '.venv', "README.md"]

html_theme = 'alabaster'
html_static_path = ['_static']
