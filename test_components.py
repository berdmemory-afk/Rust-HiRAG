#!/usr/bin/env python3

import sys
import os
import json

# Add the target directory to Python path so we can import the compiled modules
sys.path.insert(0, 'target/release')

def test_embedding_api():
    """Test the embedding API directly"""
    import subprocess
    import json
    
    print("Testing embedding API directly...")
    
    # Test the embedding API using curl
    cmd = [
        "curl", "-s", "-X", "POST",
        "https://chutes-intfloat-multilingual-e5-large.chutes.ai/v1/embeddings",
        "-H", "Authorization: Bearer cpk_37140d33ae1f4a77ba9980e4fc78a624.25e244203d585ca49b14a4bee55bfda2.MFjdI47zPJZVD16144TNJWv8xlJxBRil",
        "-H", "Content-Type: application/json",
        "-d", '{"input": "This is a test string for embedding", "model": null}'
    ]
    
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        if result.returncode == 0:
            response = json.loads(result.stdout)
            embedding_dim = len(response['data'][0]['embedding'])
            print(f"✓ Embedding API test passed - Dimension: {embedding_dim}")
            return True
        else:
            print(f"✗ Embedding API test failed - Return code: {result.returncode}")
            print(f"Error: {result.stderr}")
            return False
    except Exception as e:
        print(f"✗ Embedding API test failed - Exception: {e}")
        return False

def check_compiled_binary():
    """Check if the context manager binary was compiled successfully"""
    binary_path = "target/release/context-manager"
    if os.path.exists(binary_path):
        print("✓ Context Manager binary exists")
        return True
    else:
        print("✗ Context Manager binary not found")
        return False

def main():
    print("=== Context Manager Component Tests ===\n")
    
    # Test 1: Check if binary exists
    binary_ok = check_compiled_binary()
    
    # Test 2: Test embedding API
    embedding_ok = test_embedding_api()
    
    print("\n=== Test Summary ===")
    print(f"Binary compiled: {'✓' if binary_ok else '✗'}")
    print(f"Embedding API: {'✓' if embedding_ok else '✗'}")
    
    if binary_ok and embedding_ok:
        print("\n✓ Core components are working correctly!")
        return True
    else:
        print("\n✗ Some components need attention.")
        return False

if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)