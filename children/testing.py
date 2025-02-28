"""Script to connect to Holly and then do silly things"""

import random
import time
import ascii_dogs
import holly
import thoughts

def process_message(msg: holly.ParsedHollyMessage):
    print(msg)

    if msg.is_targeted() and msg.match("This is a very long message to make sure that no one would ever accidentally get a response from holly"):
        return "Wow that's very oddly specific, why would you say something like that?"

def main():
    """Main function"""

    parser = holly.HollyParser()

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
                        content=ret, chat_id=raw_msg.chat_id, sender=''))

        except holly.HollyError as e:
            print(f"Error: {e}")

        print('Disconnected from Holly socket')
        time.sleep(30)


if __name__ == "__main__":
    main()
