package main

import (
	"encoding/json"
	"fmt"
)

// Message represents different types of data that can be passed between actors
type Message struct {
	Type string      `json:"type"`
	Data interface{} `json:"data"`
}

// NewIntegerMessage creates an integer message
func NewIntegerMessage(value int64) Message {
	return Message{Type: "Integer", Data: value}
}

// ActorResult contains the result of actor processing
type ActorResult struct {
	Outputs map[string]Message `json:"outputs"`
	State   interface{}        `json:"state,omitempty"`
}

func main() {
	fmt.Println("Testing Go ActorResult JSON serialization...")
	
	// Create the same result structure as the counter plugin
	outputs := make(map[string]Message)
	outputs["value"] = NewIntegerMessage(42)
	
	result := ActorResult{
		Outputs: outputs,
		State:   nil,
	}
	
	fmt.Printf("Go struct: %+v\n", result)
	
	// Marshal to JSON
	data, err := json.Marshal(result)
	if err != nil {
		fmt.Printf("Error marshaling: %v\n", err)
		return
	}
	
	fmt.Printf("JSON output: %s\n", string(data))
	
	// Test what Rust expects specifically
	fmt.Println("\n--- Testing Rust expectations ---")
	
	// Parse as generic JSON to test what Rust side sees
	var genericResult map[string]interface{}
	if err := json.Unmarshal(data, &genericResult); err != nil {
		fmt.Printf("Error parsing as generic JSON: %v\n", err)
		return
	}
	
	fmt.Printf("Generic JSON parse: %+v\n", genericResult)
	
	// Check outputs field exists and is object
	if outputs, ok := genericResult["outputs"]; ok {
		fmt.Printf("✓ 'outputs' field exists: %+v\n", outputs)
		
		if outputsObj, ok := outputs.(map[string]interface{}); ok {
			fmt.Printf("✓ 'outputs' is an object with %d entries\n", len(outputsObj))
			
			for key, value := range outputsObj {
				fmt.Printf("  Key '%s': %+v\n", key, value)
				
				if valueObj, ok := value.(map[string]interface{}); ok {
					fmt.Printf("    ✓ Value is object\n")
					if msgType, ok := valueObj["type"]; ok {
						fmt.Printf("    ✓ Has 'type': %v\n", msgType)
					} else {
						fmt.Printf("    ✗ Missing 'type' field\n")
					}
					if data, ok := valueObj["data"]; ok {
						fmt.Printf("    ✓ Has 'data': %v\n", data)
					} else {
						fmt.Printf("    ✗ Missing 'data' field\n")
					}
				} else {
					fmt.Printf("    ✗ Value is not an object: %T\n", value)
				}
			}
		} else {
			fmt.Printf("✗ 'outputs' is not an object: %T\n", outputs)
		}
	} else {
		fmt.Printf("✗ 'outputs' field is missing\n")
	}
	
	// Test unmarshal to verify round-trip
	var testResult ActorResult
	if err := json.Unmarshal(data, &testResult); err != nil {
		fmt.Printf("Error unmarshaling: %v\n", err)
		return
	}
	
	fmt.Printf("Unmarshaled back: %+v\n", testResult)
}