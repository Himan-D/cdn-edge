# CDN Edge Server 🚀

A highly concurrent, low-latency Content Delivery Network (CDN) Edge Server built entirely in Rust. This project mimics the internal architecture of modern enterprise CDNs (like Cloudflare), focusing on extreme performance, robust shielding, and advanced cache management.

## 🏗 Complete Architecture

The Edge Server is designed to scale dynamically on a single machine while integrating seamlessly into a globally distributed Anycast network.

### 1. Node-Level Architecture (The Engine)
*   **Asynchronous I/O (`tokio` + `hyper`)**: Handles tens of thousands of concurrent connections using an event-driven architecture, avoiding the overhead of thread-per-request blocking.
*   **Request Coalescing (Cache Stampede Protection)**: If 10,000 users request an expired or uncached URL at the exact same millisecond, the edge node parks 9,999 of the requests in memory. It sends exactly *one* request to the origin, and upon receiving the response, broadcasts it to all 10,000 users simultaneously.
*   **`mimalloc` Memory Allocator**: Replaces the default Rust memory allocator with Microsoft's `mimalloc` to severely reduce lock contention and memory fragmentation under intense multi-threaded workloads.
*   **`SO_REUSEPORT` Multiprocessing**: Bypasses software-level threading bottlenecks by allowing multiple instances of the server to bind to the exact same port (e.g., `8080`). The Linux kernel load-balances incoming TCP connections automatically across the processes.
*   **Rate Limiting & DDoS Mitigation**: Fast in-memory atomic counters (`moka`) track user IPs (`x-forwarded-for`). If a client exceeds the configurable rate limits, they are instantly dropped with a `429 Too Many Requests`.

### 2. Network-Level Architecture (The Topology)
*   **Tiered Caching Engine**:
    *   **L1 (Memory)**: Lightning-fast LRU caching using `moka` for hot assets. Returns data in microseconds.
    *   **L2 (Disk)**: Persistent embedded key-value database (`redb`) for warm assets. If a file is found on disk, it is served to the user and immediately promoted back up to L1 memory.
*   **Dynamic Shield Load Balancing**: When the primary origin server crashes or times out, the Edge Server doesn't immediately fail. It iterates through a list of secondary "Shield Nodes". The server dynamically shuffles (`rand`) these shield nodes on every failure to guarantee that origin-failover traffic is perfectly load-balanced across the entire global shield network.
*   **Control Plane Heartbeats**: A background task constantly transmits the node's health and identity to a centralized control plane, allowing for global fleet management.

## 🔄 Request Lifecycle
1. **Connection Accepted**: The OS kernel routes the TCP connection to one of the active `SO_REUSEPORT` processes.
2. **Security Check**: The handler validates the IP against the rate-limiting atomic counters.
3. **L1 Cache Check**: Checks the `moka` memory cache.
4. **L2 Cache Check**: If L1 misses, checks the `redb` disk cache.
5. **Request Coalescing**: If both caches miss, attempts to acquire an asynchronous URL lock. If the lock is taken, it waits for the current fetch to finish.
6. **Origin Fetch**: The server fetches from the primary origin.
7. **Shield Fallback**: If the origin fails, it shuffles and queries the regional shield nodes.
8. **Cache Population**: The fetched asset is written to L2 (Disk), promoted to L1 (Memory), and streamed back to the client(s).

## 🚀 Getting Started

### Prerequisites
*   [Rust & Cargo](https://rustup.rs/)

### Running the Server
```bash
cargo run --release
```

The server configuration can be modified in `config/edge.toml`.
