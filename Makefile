.PHONY: release clean

release:
	cargo build --release
	cp target/release/chat-client ./client
	cp target/release/chat-server ./server

clean:
	cargo clean