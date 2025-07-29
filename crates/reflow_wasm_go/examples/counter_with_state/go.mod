module counter_with_state

go 1.21.0

toolchain go1.21.6

require github.com/darmie/reflow/reflow_wasm_go/sdk v0.0.0

replace github.com/darmie/reflow/reflow_wasm_go/sdk => ../../sdk

require github.com/extism/go-pdk v1.1.3 // indirect
