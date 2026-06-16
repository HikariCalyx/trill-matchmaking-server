#!/usr/bin/env python3
"""
Two-client matchmaking test. Verifies that:
  1. Offerer connects, gets Hello, sends Start (offer).
  2. Answerer connects, gets Hello, sends Start -> receives Offer.
  3. Answerer sends Answer -> offerer receives Answer.

This exercises the code path that previously deadlocked when a second
client connected to the same session.
"""

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / "target" / "python"))

import websockets
from signaling_pb2 import Packet

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8000
SESSION = "match-test"
URL = f"ws://localhost:{PORT}/?session_id={SESSION}"


async def recv_packet(ws, timeout=5):
    data = await asyncio.wait_for(ws.recv(), timeout=timeout)
    pkt = Packet()
    pkt.ParseFromString(data)
    return pkt


async def run():
    # --- Offerer connects ---
    offerer = await websockets.connect(URL)
    hello = await recv_packet(offerer)
    assert hello.WhichOneof("which") == "hello", f"offerer expected hello, got {hello.WhichOneof('which')}"
    print(f"[offerer] got hello ({len(hello.hello.ice_servers)} ICE servers)")

    # Offerer sends Start with an offer SDP
    start = Packet()
    start.start.offer_sdp = "v=0\no=offerer 1 1 IN IP4 127.0.0.1"
    start.start.connection_id = b"\x01\x02\x03\x04"
    await offerer.send(start.SerializeToString())
    print("[offerer] sent Start (offer)")

    # Give the server a moment to store the offer
    await asyncio.sleep(0.3)

    # --- Answerer connects to the SAME session ---
    answerer = await websockets.connect(URL)
    hello2 = await recv_packet(answerer)
    assert hello2.WhichOneof("which") == "hello", f"answerer expected hello, got {hello2.WhichOneof('which')}"
    print(f"[answerer] got hello ({len(hello2.hello.ice_servers)} ICE servers)")

    # Answerer sends Start -> should receive the offerer's Offer SDP
    start2 = Packet()
    start2.start.offer_sdp = ""
    start2.start.connection_id = b"\x05\x06\x07\x08"
    await answerer.send(start2.SerializeToString())
    print("[answerer] sent Start")

    offer = await recv_packet(answerer)
    assert offer.WhichOneof("which") == "offer", f"answerer expected offer, got {offer.WhichOneof('which')}"
    print(f"[answerer] received Offer SDP: {offer.offer.sdp[:30]!r}...")

    # Answerer sends Answer -> offerer should receive it
    ans = Packet()
    ans.answer.sdp = "v=0\no=answerer 2 2 IN IP4 127.0.0.1"
    await answerer.send(ans.SerializeToString())
    print("[answerer] sent Answer")

    relayed = await recv_packet(offerer)
    assert relayed.WhichOneof("which") == "answer", f"offerer expected answer, got {relayed.WhichOneof('which')}"
    print(f"[offerer] received Answer SDP: {relayed.answer.sdp[:30]!r}...")

    await offerer.close()
    await answerer.close()
    print("\nSUCCESS: full two-client matchmaking exchange completed.")


if __name__ == "__main__":
    asyncio.run(run())
