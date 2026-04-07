# Sphinx configuration for Ingot User Guide

project = 'Ingot'
author = 'Brandon Arrendondo'
copyright = '2026, Brandon Arrendondo'

extensions = []

templates_path = []
exclude_patterns = []

# -- HTML output (sphinx-rtd-theme) --
html_theme = 'sphinx_rtd_theme'
html_static_path = []

# -- LaTeX / PDF output --
latex_elements = {
    'papersize': 'letterpaper',
    'pointsize': '10pt',
    'preamble': r'''
\usepackage{enumitem}
\setlistdepth{9}
''',
}

latex_documents = [
    ('index', 'ingot-user-guide.tex', 'Ingot User Guide',
     'Brandon Arrendondo', 'manual'),
]
