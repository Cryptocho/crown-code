.PHONY: all debug release test clean

all: debug
	./build/debug/crown-code

debug:
	mkdir -p build/debug
	nimble build
	mv crown_code build/debug/crown-code

release:
	mkdir -p build/release
	nimble build -d:release
	mv crown_code build/release/crown-code

MOCK_SERVER = build/test/mock_mcp_server

$(MOCK_SERVER): tests/mock_mcp_server.nim
	mkdir -p build/test
	nim c -o:$(MOCK_SERVER) tests/mock_mcp_server.nim

test: $(MOCK_SERVER)
	@set -o pipefail; \
	export OPENROUTER_API_KEY=$$(bash -i -c 'echo $$OPENROUTER_API_KEY' 2>/dev/null); \
	start=$$(date +%s%3N); \
	script -qc "nimble test" /dev/null 2>&1 | tee /tmp/crown-test.log; \
	rc=$$?; \
	end=$$(date +%s); \
	elapsed=$$(( end * 1000 - start )); \
	pass=$$(grep -c '\[OK\]' /tmp/crown-test.log || true); \
	fail=$$(grep -c '\[FAILED\]' /tmp/crown-test.log || true); \
	skip=$$(grep -c '\[SKIPPED\]' /tmp/crown-test.log || true); \
	total=$$((pass + fail + skip)); \
	echo ""; \
	echo "Passed:  $$pass/$$total"; \
	echo "Time:    $$((elapsed / 1000))s $$((elapsed % 1000))ms"; \
	if [ "$$fail" -gt 0 ]; then exit 1; fi; \
	exit $$rc

clean:
	nimble clean
	rm -rf build/
