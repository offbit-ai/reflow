// Package reflow provides host function bindings for interacting with the Reflow runtime
package reflow

import (
	"encoding/json"
	"fmt"

	"github.com/extism/go-pdk"
)

// Host function declarations - these are imported from the Extism host
//go:wasmimport extism:host/user __send_output
func hostSendOutput(offset uint64)

//go:wasmimport extism:host/user __get_state
func hostGetState(offset uint64) uint64

//go:wasmimport extism:host/user __set_state
func hostSetState(keyOffset uint64, valueOffset uint64)

// SendOutput sends outputs asynchronously to the host
// Uses memory allocation to pass complex data structures
func SendOutput(outputs map[string]interface{}) error {
	// Convert outputs to JSON
	outputsJSON, err := json.Marshal(outputs)
	if err != nil {
		return fmt.Errorf("failed to marshal outputs: %v", err)
	}
	
	// Allocate memory for the JSON data
	mem := pdk.AllocateBytes(outputsJSON)
	defer mem.Free()
	
	// Call the host function with the memory offset
	hostSendOutput(mem.Offset())
	
	return nil
}

// SendOutputMessages sends Message outputs asynchronously to the host
// Ensures proper serialization format compatible with Rust's serde
func SendOutputMessages(outputs map[string]Message) error {
	// Convert to plain values that match Rust's expected format
	plainOutputs := make(map[string]interface{})
	for k, msg := range outputs {
		// Convert Message to plain JSON value
		plainOutputs[k] = messageToPlainValue(msg)
	}
	
	return SendOutput(plainOutputs)
}

// GetState gets a value from the actor's state
func GetState(key string) (interface{}, error) {
	// Allocate memory for the key
	keyMem := pdk.AllocateString(key)
	defer keyMem.Free()
	
	// Call the host function with the key offset
	resultOffset := hostGetState(keyMem.Offset())
	
	// If result is 0, state doesn't exist
	if resultOffset == 0 {
		return nil, nil
	}
	
	// Find the memory at the returned offset
	resultMem := pdk.FindMemory(resultOffset)
	resultBytes := resultMem.ReadBytes()
	
	// Unmarshal the JSON result
	var value interface{}
	if err := json.Unmarshal(resultBytes, &value); err != nil {
		return nil, fmt.Errorf("failed to unmarshal state value: %v", err)
	}
	
	return value, nil
}

// SetState sets a value in the actor's state
func SetState(key string, value interface{}) error {
	// Allocate memory for the key
	keyMem := pdk.AllocateString(key)
	defer keyMem.Free()
	
	// Convert value to JSON
	valueJSON, err := json.Marshal(value)
	if err != nil {
		return fmt.Errorf("failed to marshal value: %v", err)
	}
	
	// Allocate memory for the value
	valueMem := pdk.AllocateBytes(valueJSON)
	defer valueMem.Free()
	
	// Call the host function with both offsets
	hostSetState(keyMem.Offset(), valueMem.Offset())
	
	return nil
}

// Helper function to convert Message to plain value for JSON serialization
func messageToPlainValue(msg Message) interface{} {
	switch msg.Type {
	case "Flow":
		return "flow"
	case "Boolean", "Integer", "Float", "String":
		return msg.Data
	case "Event", "Object", "Array", "Optional", "Any":
		return msg.Data
	case "Stream":
		// Convert byte array to array of integers for JSON compatibility
		if bytes, ok := msg.Data.([]byte); ok {
			nums := make([]int, len(bytes))
			for i, b := range bytes {
				nums[i] = int(b)
			}
			return nums
		}
		return msg.Data
	case "Error":
		return msg.Data
	default:
		return msg.Data
	}
}