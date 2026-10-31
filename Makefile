.PHONY: check test lint

check: lint test

lint:
	cargo fmt --check
	cargo clippy --release -- -D warnings
	uv run ruff check
	uv run ruff format --check

test:
	uv run maturin develop --release -q
	uv run pytest -q
