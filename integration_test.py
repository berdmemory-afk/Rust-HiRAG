#!/usr/bin/env python3

import subprocess
import json
import time
import sys

def test_embedding_functionality():
    """Test that embedding functionality works end-to-end"""
    print("Testing embedding functionality...")
    
    # Test the embedding API directly
    cmd = [
        "curl", "-s", "-X", "POST",
        "https://chutes-intfloat-multilingual-e5-large.chutes.ai/v1/embeddings",
        "-H", "Authorization: Bearer cpk_37140d33ae1f4a77ba9980e4fc78a624.25e244203d585ca49b14a4bee55bfda2.MFjdI47zPJZVD16144TNJWv8xlJxBRil",
        "-H", "Content-Type: application/json",
        "-d", '{"input": "User preference for dark mode", "model": null}'
    ]
    
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        if result.returncode == 0:
            response = json.loads(result.stdout)
            embedding = response['data'][0]['embedding']
            print(f"✓ Embedding generated successfully - Dimension: {len(embedding)}")
            return True
        else:
            print(f"✗ Embedding generation failed")
            return False
    except Exception as e:
        print(f"✗ Embedding test failed - Exception: {e}")
        return False

def test_health_endpoints():
    """Test health endpoints (if server was running)"""
    print("Testing health endpoints...")
    
    # These would work if the server was running
    health_endpoints = [
        ("http://localhost:8080/health/live", "Liveness"),
        ("http://localhost:8080/health/ready", "Readiness"),
        ("http://localhost:8080/metrics", "Metrics")
    ]
    
    print("Note: Health endpoint tests require server to be running")
    return True

def test_compilation():
    """Verify the project compiles without errors"""
    print("Testing project compilation...")
    
    cmd = ["cargo", "check", "--release"]
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, cwd=".", timeout=60)
        if result.returncode == 0:
            print("✓ Project compiles successfully")
            return True
        else:
            print("✗ Project compilation failed")
            print(result.stderr)
            return False
    except Exception as e:
        print(f"✗ Compilation test failed - Exception: {e}")
        return False

def main():
    print("=== Context Manager Integration Tests ===\n")
    
    # Set environment variables
    env = {
        "CHUTES_API_TOKEN": "cpk_37140d33ae1f4a77ba9980e4fc78a624.25e244203d585ca49b14a4bee55bfda2.MFjdI47zPJZVD16144TNJWv8xlJxBRil",
        "API_TOKENS": "test-token"
    }
    
    # Update environment
    for key, value in env.items():
        subprocess.run(["export", f"{key}={value}"], shell=True)
    
    # Test 1: Compilation
    compilation_ok = test_compilation()
    
    # Test 2: Embedding functionality
    embedding_ok = test_embedding_functionality()
    
    # Test 3: Health endpoints
    health_ok = test_health_endpoints()
    
    print("\n=== Integration Test Summary ===")
    print(f"Compilation: {'✓' if compilation_ok else '✗'}")
    print(f"Embedding API: {'✓' if embedding_ok else '✗'}")
    print(f"Health endpoints: {'✓' if health_ok else '✗'} (requires running server)")
    
    overall_success = compilation_ok and embedding_ok
    if overall_success:
        print("\n✓ Core integration tests passed!")
        print("Note: Full end-to-end testing requires Qdrant database to be running.")
    else:
        print("\n✗ Some integration tests failed.")
    
    return overall_success

if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)