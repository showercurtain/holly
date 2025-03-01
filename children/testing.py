"""Script to connect to Holly and then do silly things"""

import random
import time
import holly


def process_message(msg: holly.ParsedHollyMessage):
    return "Nothing\nhappens"
    if msg.loose_match(["xyzzy","Xyzzy"]):
        return "Nothing\nhappens"
    else :
        return "echo: " + " ".join(msg.content)

def main():
    """Main function"""

    parser = holly.HollyParser(remove_punctuation=False,mention_name="E-Orch Park California Modesto Mission")

    while True:
        try:
            client = holly.HollyClient()
            print('Connected to Holly')
            while True:
                raw_msg = client.recv()
                print(raw_msg)
                ret = process_message(raw_msg.parse(parser))
                if ret:
                    client.send(holly.HollyMessage(
                        content=ret, chat_id=raw_msg.chat_id))

        except holly.HollyError as e:
            print(f"Error: {e}")

        print('Disconnected from Holly socket')
        time.sleep(30)


if __name__ == "__main__":
    main()
