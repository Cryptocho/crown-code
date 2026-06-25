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

test:
	nimble test

clean:
	nimble clean
	rm -rf build/
