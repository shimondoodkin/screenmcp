#!/bin/bash
set -e

cd "$(dirname "$0")"

echo "Cleaning old builds..."
rm -rf dist/ build/ src/*.egg-info

echo "Building screenmcp..."
python3 -m build

echo "Publishing to PyPI..."
python3 -m twine upload dist/*

echo "Done! Published screenmcp v$(python3 -c "import tomllib; print(tomllib.load(open('pyproject.toml','rb'))['project']['version'])")"
