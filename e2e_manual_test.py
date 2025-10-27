#!/usr/bin/env python3
"""
Comprehensive E2E Test for Context Manager
Tests all major functionality including:
1. Health checks
2. Store context (Immediate, Short-term, Long-term)
3. Search contexts
4. Delete contexts
5. Clear level
6. Metrics
"""

import requests
import json
import time
import sys

BASE_URL = "http://localhost:8081"
API_TOKEN = "test-token-12345"

def print_test(name):
    print(f"\n{'='*60}")
    print(f"TEST: {name}")
    print('='*60)

def print_result(success, message):
    status = "✓ PASS" if success else "✗ FAIL"
    print(f"{status}: {message}")
    return success

def test_health_endpoints():
    """Test all health endpoints"""
    print_test("Health Endpoints")
    
    results = []
    
    # Test /health
    try:
        response = requests.get(f"{BASE_URL}/health", timeout=5)
        data = response.json()
        results.append(print_result(
            response.status_code == 200,
            f"/health returned {response.status_code}, status: {data.get('status')}"
        ))
    except Exception as e:
        results.append(print_result(False, f"/health failed: {e}"))
    
    # Test /health/live
    try:
        response = requests.get(f"{BASE_URL}/health/live", timeout=5)
        results.append(print_result(
            response.status_code == 200,
            f"/health/live returned {response.status_code}"
        ))
    except Exception as e:
        results.append(print_result(False, f"/health/live failed: {e}"))
    
    # Test /health/ready
    try:
        response = requests.get(f"{BASE_URL}/health/ready", timeout=5)
        results.append(print_result(
            response.status_code in [200, 503],
            f"/health/ready returned {response.status_code}"
        ))
    except Exception as e:
        results.append(print_result(False, f"/health/ready failed: {e}"))
    
    return all(results)

def test_store_context():
    """Test storing contexts at different levels"""
    print_test("Store Context")
    
    results = []
    contexts = []
    
    # Test Immediate level
    try:
        payload = {
            "text": "This is an immediate context for testing",
            "level": "Immediate",
            "metadata": {
                "source": "test",
                "type": "immediate"
            },
            "priority": 1,
            "session_id": "test-session-1"
        }
        
        response = requests.post(
            f"{BASE_URL}/api/v1/contexts",
            headers={"Authorization": f"Bearer {API_TOKEN}"},
            json=payload,
            timeout=10
        )
        
        if response.status_code in [200, 201]:
            data = response.json()
            context_id = data.get("id")
            contexts.append(("Immediate", context_id))
            results.append(print_result(True, f"Stored Immediate context: {context_id}"))
        else:
            results.append(print_result(False, f"Failed to store Immediate context: {response.status_code} - {response.text}"))
    except Exception as e:
        results.append(print_result(False, f"Immediate context error: {e}"))
    
    # Test Short-term level
    try:
        payload = {
            "text": "This is a short-term context for testing with more detailed information",
            "level": "ShortTerm",
            "metadata": {
                "source": "test",
                "type": "shortterm"
            },
            "priority": 2,
            "session_id": "test-session-1"
        }
        
        response = requests.post(
            f"{BASE_URL}/api/v1/contexts",
            headers={"Authorization": f"Bearer {API_TOKEN}"},
            json=payload,
            timeout=10
        )
        
        if response.status_code in [200, 201]:
            data = response.json()
            context_id = data.get("id")
            contexts.append(("ShortTerm", context_id))
            results.append(print_result(True, f"Stored ShortTerm context: {context_id}"))
        else:
            results.append(print_result(False, f"Failed to store ShortTerm context: {response.status_code} - {response.text}"))
    except Exception as e:
        results.append(print_result(False, f"ShortTerm context error: {e}"))
    
    # Test Long-term level
    try:
        payload = {
            "text": "This is a long-term context for testing with comprehensive information about the system",
            "level": "LongTerm",
            "metadata": {
                "source": "test",
                "type": "longterm"
            },
            "priority": 3,
            "session_id": "test-session-1"
        }
        
        response = requests.post(
            f"{BASE_URL}/api/v1/contexts",
            headers={"Authorization": f"Bearer {API_TOKEN}"},
            json=payload,
            timeout=10
        )
        
        if response.status_code in [200, 201]:
            data = response.json()
            context_id = data.get("id")
            contexts.append(("LongTerm", context_id))
            results.append(print_result(True, f"Stored LongTerm context: {context_id}"))
        else:
            results.append(print_result(False, f"Failed to store LongTerm context: {response.status_code} - {response.text}"))
    except Exception as e:
        results.append(print_result(False, f"LongTerm context error: {e}"))
    
    return all(results), contexts

def test_search_contexts():
    """Test searching contexts"""
    print_test("Search Contexts")
    
    results = []
    
    # Search for "testing"
    try:
        payload = {
            "query": "testing information",
            "level": None,
            "limit": 10,
            "max_tokens": 4000
        }
        
        response = requests.post(
            f"{BASE_URL}/api/v1/contexts/search",
            headers={"Authorization": f"Bearer {API_TOKEN}"},
            json=payload,
            timeout=10
        )
        
        if response.status_code == 200:
            data = response.json()
            contexts = data.get("contexts", [])
            results.append(print_result(
                len(contexts) > 0,
                f"Found {len(contexts)} contexts matching 'testing information'"
            ))
            
            # Print some details
            for ctx in contexts[:3]:
                print(f"  - ID: {ctx.get('id')}, Level: {ctx.get('level')}, Score: {ctx.get('score', 0):.4f}")
        else:
            results.append(print_result(False, f"Search failed: {response.status_code} - {response.text}"))
    except Exception as e:
        results.append(print_result(False, f"Search error: {e}"))
    
    # Search with level filter
    try:
        payload = {
            "query": "context",
            "level": "Immediate",
            "limit": 5,
            "max_tokens": 4000
        }
        
        response = requests.post(
            f"{BASE_URL}/api/v1/contexts/search",
            headers={"Authorization": f"Bearer {API_TOKEN}"},
            json=payload,
            timeout=10
        )
        
        if response.status_code == 200:
            data = response.json()
            contexts = data.get("contexts", [])
            results.append(print_result(
                True,
                f"Level-filtered search returned {len(contexts)} Immediate contexts"
            ))
        else:
            results.append(print_result(False, f"Level-filtered search failed: {response.status_code}"))
    except Exception as e:
        results.append(print_result(False, f"Level-filtered search error: {e}"))
    
    return all(results)

def test_delete_context(contexts):
    """Test deleting a specific context"""
    print_test("Delete Context")
    
    if not contexts:
        return print_result(False, "No contexts available to delete")
    
    results = []
    level, context_id = contexts[0]
    
    try:
        payload = {
            "id": context_id,
            "level": level
        }
        
        response = requests.post(
            f"{BASE_URL}/api/v1/contexts/delete",
            headers={"Authorization": f"Bearer {API_TOKEN}"},
            json=payload,
            timeout=10
        )
        
        if response.status_code in [200, 204]:
            results.append(print_result(True, f"Deleted context {context_id} from {level}"))
        else:
            results.append(print_result(False, f"Delete failed: {response.status_code} - {response.text}"))
    except Exception as e:
        results.append(print_result(False, f"Delete error: {e}"))
    
    return all(results)

def test_clear_level():
    """Test clearing a context level"""
    print_test("Clear Level")
    
    results = []
    
    try:
        payload = "Immediate"
        
        response = requests.post(
            f"{BASE_URL}/api/v1/contexts/clear",
            headers={"Authorization": f"Bearer {API_TOKEN}"},
            json=payload,
            timeout=10
        )
        
        if response.status_code == 200:
            data = response.json()
            results.append(print_result(True, f"Cleared Immediate level: {data.get('message')}"))
        else:
            results.append(print_result(False, f"Clear failed: {response.status_code} - {response.text}"))
    except Exception as e:
        results.append(print_result(False, f"Clear error: {e}"))
    
    return all(results)

def test_metrics():
    """Test metrics endpoint"""
    print_test("Metrics")
    
    results = []
    
    try:
        response = requests.get(f"{BASE_URL}/metrics", timeout=5)
        
        if response.status_code == 200:
            metrics_text = response.text
            
            # Check for key metrics
            has_requests = "context_manager_requests_total" in metrics_text
            has_errors = "context_manager_errors_total" in metrics_text
            has_latency = "context_manager_request_duration_ms" in metrics_text or "context_manager_embedding_duration_ms" in metrics_text
            
            results.append(print_result(has_requests, "Metrics contain request counters"))
            results.append(print_result(has_errors, "Metrics contain error counters"))
            results.append(print_result(has_latency, "Metrics contain latency histograms"))
            
            # Print sample metrics
            lines = metrics_text.split('\n')
            print("\nSample metrics:")
            for line in lines[:10]:
                if line and not line.startswith('#'):
                    print(f"  {line}")
        else:
            results.append(print_result(False, f"Metrics failed: {response.status_code}"))
    except Exception as e:
        results.append(print_result(False, f"Metrics error: {e}"))
    
    return all(results)

def test_authentication():
    """Test authentication middleware"""
    print_test("Authentication")
    
    results = []
    
    # Test without token
    try:
        response = requests.post(
            f"{BASE_URL}/api/v1/contexts",
            json={"text": "test", "level": "Immediate"},
            timeout=5
        )
        results.append(print_result(
            response.status_code == 401,
            f"Request without token rejected with {response.status_code}"
        ))
    except Exception as e:
        results.append(print_result(False, f"Auth test error: {e}"))
    
    # Test with invalid token
    try:
        response = requests.post(
            f"{BASE_URL}/api/v1/contexts",
            headers={"Authorization": "Bearer invalid-token"},
            json={"text": "test", "level": "Immediate"},
            timeout=5
        )
        results.append(print_result(
            response.status_code == 401,
            f"Request with invalid token rejected with {response.status_code}"
        ))
    except Exception as e:
        results.append(print_result(False, f"Invalid token test error: {e}"))
    
    return all(results)

def main():
    print("\n" + "="*60)
    print("CONTEXT MANAGER E2E TEST SUITE")
    print("="*60)
    
    all_results = []
    
    # Run tests
    all_results.append(("Health Endpoints", test_health_endpoints()))
    all_results.append(("Authentication", test_authentication()))
    
    store_success, contexts = test_store_context()
    all_results.append(("Store Context", store_success))
    
    time.sleep(1)  # Give time for indexing
    
    all_results.append(("Search Contexts", test_search_contexts()))
    all_results.append(("Delete Context", test_delete_context(contexts)))
    all_results.append(("Clear Level", test_clear_level()))
    all_results.append(("Metrics", test_metrics()))
    
    # Summary
    print("\n" + "="*60)
    print("TEST SUMMARY")
    print("="*60)
    
    passed = sum(1 for _, result in all_results if result)
    total = len(all_results)
    
    for name, result in all_results:
        status = "✓ PASS" if result else "✗ FAIL"
        print(f"{status}: {name}")
    
    print(f"\nTotal: {passed}/{total} tests passed")
    
    if passed == total:
        print("\n🎉 ALL TESTS PASSED!")
        return 0
    else:
        print(f"\n❌ {total - passed} test(s) failed")
        return 1

if __name__ == "__main__":
    sys.exit(main())