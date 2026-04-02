install: install-cdt install-rippy

install-cdt:
	cargo install --path crates/cdt

install-rippy:
	cargo install --path crates/rippy

uninstall:
	cargo uninstall cdt rippy
