.PHONY: all build

DEPS := .venv

all: build ## everything
	@echo "make build is what you want"

build: ## build the thing
	uv run fixture build --fast
	$(MAKE) report

report:
	uv run fixture report

window: ## the graphical program
	uv run fixture-window
