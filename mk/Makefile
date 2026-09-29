# Единый Makefile (делегирует в подфайлы)

.PHONY: help check test core frontend tauri clean

help:
	@echo "Gesture Control — Make targets"
	@echo ""
	@echo "Общие:"
	@echo "  check     - все проверки (tsc + vite + test + clippy)"
	@echo "  test      - cargo test --workspace --offline"
	@echo "  clean     - cargo clean"
	@echo ""
	@echo "Разделы:"
	@echo "  core      - см. mk/Makefile.core"
	@echo "  frontend  - см. mk/Makefile.frontend"
	@echo "  tauri     - см. mk/Makefile.tauri"

check:
	@$(MAKE) -f mk/Makefile.core check

test:
	@$(MAKE) -f mk/Makefile.core test

clean:
	@$(MAKE) -f mk/Makefile.core clean

core:
	@$(MAKE) -f mk/Makefile.core help

frontend:
	@$(MAKE) -f mk/Makefile.frontend help

tauri:
	@$(MAKE) -f mk/Makefile.tauri help
