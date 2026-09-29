# Общие цвета и помощники для всех Makefile.
# Подключается через `include $(ROOT)/mk/colors.mk`.

# Корень репозитория считается от пути самого colors.mk, поэтому не зависит
# от того, какой Makefile его подключил.
HERE  := $(patsubst %/,%,$(dir $(abspath $(lastword $(MAKEFILE_LIST)))))
ROOT  := $(abspath $(HERE)/..)

# Цвета включаются только когда вывод make идёт в терминал.
# Проверять `[ -t 1 ]` внутри $(shell) бесполезно: во время подстановки stdout
# make всегда pipe, поэтому такая проверка всегда ложна. GNU Make 4.4
# сообщает терминал вывода в MAKE_TERMOUT — он пуст при перенаправлении.
# Уважается NO_COLOR. Используются сырые ANSI-коды, а не tput: tput
# подмешивает служебные последовательности terminfo вида ESC(B.
ifneq ($(NO_COLOR),)
  HAS_COLOR :=
else ifneq ($(strip $(MAKE_TERMOUT)),)
  HAS_COLOR := yes
else
  HAS_COLOR :=
endif

ifneq ($(strip $(HAS_COLOR)),)
  BOLD    := $(shell printf '\033[1m')
  DIM     := $(shell printf '\033[2m')
  RESET   := $(shell printf '\033[0m')
  RED     := $(shell printf '\033[31m')
  GREEN   := $(shell printf '\033[32m')
  YELLOW  := $(shell printf '\033[33m')
  BLUE    := $(shell printf '\033[34m')
  MAGENTA := $(shell printf '\033[35m')
  CYAN    := $(shell printf '\033[36m')
else
  BOLD :=
  DIM :=
  RESET :=
  RED :=
  GREEN :=
  YELLOW :=
  BLUE :=
  MAGENTA :=
  CYAN :=
endif

TAURI_BIN := $(ROOT)/frontend/node_modules/.bin/tauri
APP_BIN   := $(ROOT)/src-tauri/target/release/gesture-control

# banner <заголовок>
define banner
	@printf '\n$(BOLD)$(CYAN)━━ $(1) $(RESET)$(BOLD)$(DIM)━━━━━━━━━━━━━━━━━━━━━━━━━━━━$(RESET)\n'
endef

# step <команда> <описание>
define step
	@printf '$(BOLD)$(BLUE)▶$(RESET) $(2)\n'
	@$(1)
endef

# done <текст>
define done
	@printf '$(BOLD)$(GREEN)✓$(RESET) $(1)\n'
endef

# warn <текст>
define warn
	@printf '$(BOLD)$(YELLOW)⚠$(RESET) $(1)\n'
endef

# fail <текст>
define fail
	@printf '$(BOLD)$(RED)✗$(RESET) $(1)\n' >&2; exit 1
endef

# delegate <файл> — вызов раздела с теми же целями, что и у вызывающего.
define delegate
	@$(MAKE) --no-print-directory -f $(ROOT)/mk/$(1) $(MAKECMDGOALS)
endef

# section <раздел> — голая цель-раздел показывает справку, а `make core check`
# выполняет в разделе check. Имя раздела из целей убирается.
define section
	@$(MAKE) --no-print-directory -f $(ROOT)/mk/Makefile.$(1) $(filter-out $(1),$(MAKECMDGOALS))
endef
