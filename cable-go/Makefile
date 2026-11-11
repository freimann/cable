APP     := cable
DIST    := dist
LDFLAGS := -s -w
TARGETS := darwin/amd64 darwin/arm64 linux/amd64 linux/arm64 windows/amd64 windows/arm64

.PHONY: all build test vet dist clean

all: build

build:
	CGO_ENABLED=0 go build -trimpath -ldflags "$(LDFLAGS)" -o $(APP) .

test:
	go test ./...

vet:
	go vet ./...

dist: test
	@mkdir -p $(DIST)
	@set -e; for target in $(TARGETS); do \
		os=$${target%/*}; arch=$${target#*/}; ext=; \
		if [ "$$os" = "windows" ]; then ext=.exe; fi; \
		out=$(DIST)/$(APP)-$$os-$$arch$$ext; \
		echo "building $$out"; \
		CGO_ENABLED=0 GOOS=$$os GOARCH=$$arch go build -trimpath -ldflags "$(LDFLAGS)" -o $$out .; \
	done
	@ls -lh $(DIST)

clean:
	rm -f $(APP) $(APP).exe
	rm -rf $(DIST)
