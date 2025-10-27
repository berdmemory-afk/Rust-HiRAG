import requests
import json
import os

# Test the embedding API directly
CHUTES_API_TOKEN = "cpk_37140d33ae1f4a77ba9980e4fc78a624.25e244203d585ca49b14a4bee55bfda2.MFjdI47zPJZVD16144TNJWv8xlJxBRil"

def test_embedding_api():
    url = "https://chutes-intfloat-multilingual-e5-large.chutes.ai/v1/embeddings"
    headers = {
        "Authorization": f"Bearer {CHUTES_API_TOKEN}",
        "Content-Type": "application/json"
    }
    data = {
        "input": "This is a test string for embedding",
        "model": None
    }
    
    response = requests.post(url, headers=headers, json=data)
    print(f"Status Code: {response.status_code}")
    print(f"Response: {response.text}")
    
    if response.status_code == 200:
        result = response.json()
        print(f"Embedding dimension: {len(result['data'][0]['embedding'])}")
        return True
    else:
        print("Failed to get embedding")
        return False

if __name__ == "__main__":
    test_embedding_api()