# Главный Makefile: сводка разделов и запуск всего.
#
#   make            - эта справка
#   make dev        - горячая перезагрузка приложения
#   make release    - релизная сборка и запуск
#   make check      - все проверки качества

include $(patsubst %/,%,$(dir $(abspath $(lastword $(MAKEFILE_LIST)))))/colors.mk

.DEFAULT_GOAL := help

.PHONY: help check typecheck test clippy fmt core frontend tauri dev release build preview clean

help:
	@printf '\n$(BOLD)$(CYAN)Управление жестами$(RESET) $(DIM)— цели make$(RESET)\n\n'
	@printf '$(BOLD)Запуск$(RESET)\n'
	@printf '  $(BOLD)$(MAGENTA)dev$(RESET)        горячая перезагрузка: Rust и фронтенд\n'
	@printf '  $(BOLD)$(MAGENTA)release$(RESET)    релизная сборка и запуск бинаря\n'
	@printf '  $(BOLD)$(MAGENTA)build$(RESET)      установщик (.deb, .AppImage)\n'
	@printf '  $(BOLD)$(MAGENTA)preview$(RESET)    предпросмотр собранного фронтенда\n'
	@printf '\n$(BOLD)Проверки$(RESET)\n'
	@printf '  $(BOLD)check$(RESET)      tsc, cargo test, clippy\n'
	@printf '  $(BOLD)typecheck$(RESET)  tsc --noEmit\n'
	@printf '  $(BOLD)test$(RESET)       cargo test --workspace\n'
	@printf '  $(BOLD)clippy$(RESET)     cargo clippy --all-targets\n'
	@printf '  $(BOLD)fmt$(RESET)        cargo fmt --check\n'
	@printf '\n$(BOLD)Разделы$(RESET) $(DIM)(свои цели: make core --help)$(RESET)\n'
	@printf '  $(BOLD)core$(RESET)       ядро и мосты\n'
	@printf '  $(BOLD)frontend$(RESET)   React, TypeScript, Vite\n'
	@printf '  $(BOLD)tauri$(RESET)      десктопное окно\n'
	@printf '\n$(BOLD)Прочее$(RESET)\n'
	@printf '  $(BOLD)clean$(RESET)      очистка артефактов сборки\n'
	@printf '\n$(DIM)Примеры: make check, make core test, make frontend build$(RESET)\n\n'

check typecheck test clippy fmt clean:
	$(call delegate,Makefile.core)

dev release build:
	$(call delegate,Makefile.tauri)

preview:
	$(call delegate,Makefile.frontend)

# `make core` -> help раздела, `make core check` -> check раздела.
core frontend tauri:
	$(call section,$@)
