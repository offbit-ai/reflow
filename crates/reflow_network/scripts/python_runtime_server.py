#!/usr/bin/env python3
"""
Python Runtime Server for Reflow Script Actors

This server implements a WebSocket RPC interface for executing Python-based
actors in the Reflow network. It handles actor loading, execution, and
bidirectional communication with the Reflow system.
"""

import asyncio
import json
import sys
import os
import importlib.util
import traceback
from typing import Dict, Any, Optional, Callable
from dataclasses import dataclass
import websockets
import uuid
import logging
from datetime import datetime

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

@dataclass
class ActorContext:
    """Context provided to actor functions"""
    payload: Dict[str, Any]
    config: Dict[str, Any]
    state: Dict[str, Any]
    actor_id: str
    timestamp: int
    
    def get_input(self, port: str, default=None):
        """Get input from a specific port"""
        return self.payload.get(port, default)
    
    def send_async_output(self, port: str, data: Any):
        """Send asynchronous output (handled by runtime)"""
        # This will be intercepted by the runtime
        pass

class Message:
    """Message helper class for actors"""
    
    @staticmethod
    def string(value: str) -> Dict[str, Any]:
        return {"type": "string", "value": value}
    
    @staticmethod
    def integer(value: int) -> Dict[str, Any]:
        return {"type": "integer", "value": value}
    
    @staticmethod
    def float(value: float) -> Dict[str, Any]:
        return {"type": "float", "value": value}
    
    @staticmethod
    def boolean(value: bool) -> Dict[str, Any]:
        return {"type": "boolean", "value": value}
    
    @staticmethod
    def object(value: Dict) -> Dict[str, Any]:
        return {"type": "object", "value": value}
    
    @staticmethod
    def array(value: list) -> Dict[str, Any]:
        return {"type": "array", "value": value}
    
    @staticmethod
    def error(message: str) -> Dict[str, Any]:
        return {"type": "error", "value": message}

class PythonRuntimeServer:
    """WebSocket RPC server for Python actors"""
    
    def __init__(self, host: str = "127.0.0.1", port: int = 8765):
        self.host = host
        self.port = port
        self.actors: Dict[str, Callable] = {}
        self.websocket = None
        self.running = False
        
    async def load_actor(self, file_path: str) -> bool:
        """Load an actor from a Python file"""
        try:
            # Load the module
            spec = importlib.util.spec_from_file_location("actor_module", file_path)
            if spec is None or spec.loader is None:
                logger.error(f"Failed to load module spec for {file_path}")
                return False
                
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            
            # Find the actor function
            actor_func = None
            actor_name = None
            
            # Look for decorated function
            for name, obj in module.__dict__.items():
                if callable(obj) and hasattr(obj, '__actor_metadata__'):
                    actor_func = obj
                    actor_name = obj.__actor_metadata__.get('name', name)
                    break
            
            # Fallback to finding async function
            if actor_func is None:
                for name, obj in module.__dict__.items():
                    if callable(obj) and asyncio.iscoroutinefunction(obj):
                        actor_func = obj
                        actor_name = name
                        break
            
            if actor_func:
                self.actors[actor_name] = actor_func
                logger.info(f"Loaded actor '{actor_name}' from {file_path}")
                return True
            else:
                logger.error(f"No actor function found in {file_path}")
                return False
                
        except Exception as e:
            logger.error(f"Failed to load actor from {file_path}: {e}")
            return False
    
    async def handle_rpc_request(self, request: Dict[str, Any]) -> Dict[str, Any]:
        """Handle an RPC request"""
        request_id = request.get('id')
        method = request.get('method')
        params = request.get('params', {})
        
        try:
            if method == 'process':
                # Execute actor
                result = await self.process_actor(params)
                return {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": result
                }
            elif method == 'load':
                # Load a new actor
                file_path = params.get('file_path')
                success = await self.load_actor(file_path)
                return {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {"success": success}
                }
            elif method == 'list':
                # List loaded actors
                return {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {"actors": list(self.actors.keys())}
                }
            else:
                # Unknown method
                return {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {
                        "code": -32601,
                        "message": f"Method not found: {method}"
                    }
                }
        except Exception as e:
            logger.error(f"Error handling RPC request: {e}")
            return {
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {
                    "code": -32603,
                    "message": str(e),
                    "data": traceback.format_exc()
                }
            }
    
    async def process_actor(self, params: Dict[str, Any]) -> Dict[str, Any]:
        """Process an actor request"""
        actor_id = params.get('actor_id')
        
        # Find the actor
        actor_func = self.actors.get(actor_id)
        if not actor_func:
            raise ValueError(f"Actor not found: {actor_id}")
        
        # Create context
        context = ActorContext(
            payload=params.get('payload', {}),
            config=params.get('config', {}),
            state=params.get('state', {}),
            actor_id=actor_id,
            timestamp=params.get('timestamp', 0)
        )
        
        # Hook for async outputs
        original_send = context.send_async_output
        async_outputs = []
        
        def capture_async_output(port: str, data: Any):
            async_outputs.append((port, data))
            # Send as notification immediately
            if self.websocket:
                asyncio.create_task(self.send_notification(
                    "output",
                    {
                        "actor_id": actor_id,
                        "port": port,
                        "data": data,
                        "timestamp": int(datetime.now().timestamp() * 1000)
                    }
                ))
        
        context.send_async_output = capture_async_output
        
        # Execute actor
        try:
            if asyncio.iscoroutinefunction(actor_func):
                result = await actor_func(context)
            else:
                result = actor_func(context)
            
            # Format outputs
            outputs = {}
            if isinstance(result, dict):
                outputs = result
            elif result is not None:
                outputs = {"output": result}
            
            return {"outputs": outputs}
            
        except Exception as e:
            logger.error(f"Actor execution failed: {e}")
            return {
                "error": str(e),
                "traceback": traceback.format_exc()
            }
    
    async def send_notification(self, method: str, params: Any):
        """Send a JSON-RPC notification"""
        if self.websocket:
            notification = {
                "jsonrpc": "2.0",
                "method": method,
                "params": params
            }
            try:
                await self.websocket.send(json.dumps(notification))
            except Exception as e:
                logger.error(f"Failed to send notification: {e}")
    
    async def handle_connection(self, websocket, path):
        """Handle a WebSocket connection"""
        self.websocket = websocket
        logger.info(f"Client connected from {websocket.remote_address}")
        
        try:
            async for message in websocket:
                try:
                    request = json.loads(message)
                    logger.debug(f"Received request: {request}")
                    
                    # Handle the request
                    response = await self.handle_rpc_request(request)
                    
                    # Send response
                    await websocket.send(json.dumps(response))
                    logger.debug(f"Sent response: {response}")
                    
                except json.JSONDecodeError as e:
                    logger.error(f"Invalid JSON: {e}")
                    error_response = {
                        "jsonrpc": "2.0",
                        "id": None,
                        "error": {
                            "code": -32700,
                            "message": "Parse error"
                        }
                    }
                    await websocket.send(json.dumps(error_response))
                    
        except websockets.exceptions.ConnectionClosed:
            logger.info("Client disconnected")
        except Exception as e:
            logger.error(f"Connection error: {e}")
        finally:
            self.websocket = None
    
    async def start(self):
        """Start the WebSocket server"""
        logger.info(f"Starting Python Runtime Server on {self.host}:{self.port}")
        self.running = True
        
        async with websockets.serve(self.handle_connection, self.host, self.port):
            logger.info(f"Server listening on ws://{self.host}:{self.port}")
            await asyncio.Future()  # Run forever

def main():
    """Main entry point"""
    import argparse
    
    parser = argparse.ArgumentParser(description='Python Runtime Server for Reflow')
    parser.add_argument('--host', default='127.0.0.1', help='Host to bind to')
    parser.add_argument('--port', type=int, default=8765, help='Port to bind to')
    parser.add_argument('--load', nargs='+', help='Actor files to preload')
    parser.add_argument('--debug', action='store_true', help='Enable debug logging')
    
    args = parser.parse_args()
    
    if args.debug:
        logging.getLogger().setLevel(logging.DEBUG)
    
    # Create server
    server = PythonRuntimeServer(args.host, args.port)
    
    # Preload actors if specified
    if args.load:
        async def preload():
            for file_path in args.load:
                await server.load_actor(file_path)
        asyncio.run(preload())
    
    # Start server
    try:
        asyncio.run(server.start())
    except KeyboardInterrupt:
        logger.info("Server stopped by user")

if __name__ == '__main__':
    main()