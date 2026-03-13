#!/usr/bin/env python3
"""
Test Python actor for MicroSandbox integration
Tests Message serialization, compression, and bidirectional communication
"""

import asyncio
import json

# Actor decorator for metadata
def actor(func):
    func.__actor_metadata__ = True
    return func

@actor
async def process(context):
    """
    Main actor processing function
    Tests various Message types and operations
    """
    print(f"Processing in Python actor: {context.actor_id}")
    
    # Test getting various input types
    integer_input = context.get_input("integer_port", 0)
    string_input = context.get_input("string_port", "")
    array_input = context.get_input("array_port", [])
    object_input = context.get_input("object_port", {})
    binary_input = context.get_input("binary_port", b"")
    
    print(f"Received inputs: int={integer_input}, str={string_input}, arr={array_input}")
    
    # Test async output sending
    await context.send_output("echo_int", integer_input * 2)
    await context.send_output("echo_str", string_input.upper() if string_input else "NO_STRING")
    
    # Test large data compression (>1KB)
    large_data = "x" * 2000  # 2KB of data
    await context.send_output("compressed", large_data)
    
    # Test array processing
    if array_input:
        processed_array = [item * 2 if isinstance(item, (int, float)) else item for item in array_input]
        await context.send_output("processed_array", processed_array)
    
    # Test object manipulation
    if object_input:
        result_object = {
            **object_input,
            "processed": True,
            "timestamp": context.timestamp
        }
        await context.send_output("processed_object", result_object)
    
    # Test binary data handling
    if binary_input:
        # Convert bytes to list for JSON serialization
        if isinstance(binary_input, bytes):
            binary_list = list(binary_input)
        else:
            binary_list = binary_input
        await context.send_output("binary_size", len(binary_list))
    
    # Test state persistence (if Redis is available)
    try:
        counter = await context.get_state("counter", 0)
        new_counter = counter + 1
        await context.set_state("counter", new_counter)
        await context.send_output("state_counter", new_counter)
    except Exception as e:
        print(f"State persistence not available: {e}")
    
    # Return final outputs
    return {
        "status": "success",
        "actor_id": context.actor_id,
        "input_count": len(context.payload),
        "outputs_sent": len(context._outputs)
    }

@actor
async def stream_processor(context):
    """
    Test streaming/progressive outputs
    """
    iterations = context.get_input("iterations", 5)
    delay = context.get_input("delay", 0.1)
    
    for i in range(iterations):
        # Send progress updates
        progress = {
            "current": i + 1,
            "total": iterations,
            "percent": ((i + 1) / iterations) * 100
        }
        await context.send_output("progress", progress)
        
        # Process chunk
        chunk_data = f"Chunk {i}: " + ("data" * 100)  # ~400 bytes per chunk
        await context.send_output("chunk", chunk_data)
        
        # Small delay to simulate processing
        await asyncio.sleep(delay)
    
    # Send completion
    await context.send_output("complete", True)
    
    return {
        "total_chunks": iterations,
        "status": "completed"
    }

@actor
async def error_handler(context):
    """
    Test error handling
    """
    should_error = context.get_input("should_error", False)
    error_message = context.get_input("error_message", "Test error")
    
    if should_error:
        raise Exception(error_message)
    
    return {
        "status": "no_error",
        "message": "Executed successfully"
    }

# Metadata for the actor
__actor_metadata__ = {
    "component": "python_test_actor",
    "description": "Python test actor for MicroSandbox integration",
    "version": "1.0.0",
    "inports": [
        "integer_port",
        "string_port", 
        "array_port",
        "object_port",
        "binary_port",
        "iterations",
        "delay",
        "should_error",
        "error_message"
    ],
    "outports": [
        "echo_int",
        "echo_str",
        "compressed",
        "processed_array",
        "processed_object",
        "binary_size",
        "state_counter",
        "progress",
        "chunk",
        "complete",
        "error"
    ]
}