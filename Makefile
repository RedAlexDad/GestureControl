# Корневой Makefile — тонкая обёртка над mk/.
# Всё живёт в mk/, здесь только переадресация, чтобы `make` работал из корня.

include $(patsubst %/,%,$(dir $(abspath $(lastword $(MAKEFILE_LIST)))))/mk/colors.mk

.DEFAULT_GOAL := help

.PHONY: help check typecheck test clippy fmt core frontend tauri dev release build preview clean

help:
	@$(MAKE) --no-print-directory -f $(ROOT)/mk/Makefile help

check typecheck test clippy fmt clean dev release build preview:
	@$(MAKE) --no-print-directory -f $(ROOT)/mk/Makefile $@

# Правила-разделы: `make core` показывает справку раздела,
# `make core check` выполняет в нём check.
core frontend tauri:
	@$(MAKE) --no-print-directory -f $(ROOT)/mk/Makefile $@
