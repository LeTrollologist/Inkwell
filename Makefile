TAG ?= v0.1.1
PYTHON ?= python

.PHONY: release draft package build test check installer clean

release:
	$(PYTHON) scripts/pipeline.py $(TAG)

draft:
	$(PYTHON) scripts/pipeline.py $(TAG) --draft

package:
	$(PYTHON) scripts/pipeline.py $(TAG) --no-publish

build:
	cargo build --release --locked

installer:
	$(PYTHON) scripts/pipeline.py $(TAG) --no-publish

test:
	cargo test --locked
	node scripts/check-ui.js

check: test

clean:
	$(PYTHON) -c "import shutil; shutil.rmtree('dist', ignore_errors=True)"
	cargo clean
