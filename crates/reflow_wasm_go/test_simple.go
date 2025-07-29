package main

import (
	"encoding/json"
	"fmt"

	reflow "github.com/darmie/reflow/reflow_wasm_go/sdk"
)

func main() {
	fmt.Println("Testing simplified Go ActorResult JSON...")

	// Create exact same structure as counter plugin
	outputs := make(map[string]reflow.Message)
	outputs["value"] = reflow.NewIntegerMessage(42)

	result := reflow.ActorResult{
		Outputs: outputs,
		State:   nil,
	}

	// Marshal to JSON 
	data, err := json.Marshal(result)
	if err != nil {
		fmt.Printf("❌ JSON marshal error: %v\n", err)
		return
	}

	fmt.Printf("📝 Raw JSON: %s\n", string(data))

	// Test what Rust side will see
	var rustView map[string]interface{}
	if err := json.Unmarshal(data, &rustView); err != nil {
		fmt.Printf("❌ Error parsing JSON: %v\n", err)
		return
	}

	fmt.Printf("🔍 Rust view: %+v\n", rustView)

	// Check for "outputs" field specifically
	if outputs, exists := rustView["outputs"]; exists {
		fmt.Printf("✅ 'outputs' field exists\n")
		
		if outputsMap, ok := outputs.(map[string]interface{}); ok {
			fmt.Printf("✅ 'outputs' is object with %d keys\n", len(outputsMap))
			
			if value, hasValue := outputsMap["value"]; hasValue {
				fmt.Printf("✅ 'value' key exists: %+v\n", value)
				
				if valueMap, ok := value.(map[string]interface{}); ok {
					fmt.Printf("✅ 'value' is object\n")
					if msgType, ok := valueMap["type"]; ok {
						fmt.Printf("✅ Has 'type': %v\n", msgType)
					}
					if data, ok := valueMap["data"]; ok {
						fmt.Printf("✅ Has 'data': %v\n", data)
						if dataNum, ok := data.(float64); ok {
							fmt.Printf("✅ Data is number: %v\n", dataNum)
						}
					}
				} else {
					fmt.Printf("❌ 'value' is not object: %T\n", value)
				}
			} else {
				fmt.Printf("❌ No 'value' key found\n")
			}
		} else {
			fmt.Printf("❌ 'outputs' is not object: %T\n", outputs)
		}
	} else {
		fmt.Printf("❌ No 'outputs' field found\n")
	}

	fmt.Println("\n✅ Test completed - if all checks passed, the JSON structure is correct!")
}