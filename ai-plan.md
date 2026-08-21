Enhancement:
- Clustering (https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2) -> User manual taxonomy -> Supervised learning

Beside title, url, the browser extension can crawl the webpage and extract important information then sent to the app.

Take a look at https://www.ibm.com/think/topics/zero-shot-learning

```
App + domain + window title
          │
          ▼
   Known-domain rules ──────────► category
          │ no match
          ▼
   Text embedding model
          │
          ▼
cosine similarity against
category descriptions/examples
          │
     ┌────┴────┐
 high confidence  low confidence
     │               │
 category       Uncategorized
```