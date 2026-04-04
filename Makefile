install: install-cdt install-rippy

install-cdt:
	cargo install --path crates/cdt

install-rippy:
	cargo install --path crates/rippy

uninstall:
	cargo uninstall cdt rippy

setup:
	git config core.hooksPath .githooks
	@echo "Git hooks enabled."

check:
	cargo fmt --check
	cargo clippy --workspace -- -D warnings
	cargo test --workspace
