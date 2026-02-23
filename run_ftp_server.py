import os
from pyftpdlib.authorizers import DummyAuthorizer
from pyftpdlib.handlers import FTPHandler
from pyftpdlib.servers import FTPServer

# Create a test directory and some dummy files
TEST_DIR = os.path.join(os.getcwd(), 'ftp_test_dir')
os.makedirs(TEST_DIR, exist_ok=True)

# Create a sample text file
with open(os.path.join(TEST_DIR, 'hello.txt'), 'w') as f:
    f.write('Hello from the local FTP server!')

# Create a sample directory
os.makedirs(os.path.join(TEST_DIR, 'test_folder'), exist_ok=True)
with open(os.path.join(TEST_DIR, 'test_folder', 'nested.md'), 'w') as f:
    f.write('# This is a nested file')

def main():
    # Instantiate a dummy authorizer for managing 'virtual' users
    authorizer = DummyAuthorizer()

    # Define a new user having full r/w permissions
    authorizer.add_user('user', 'pass', TEST_DIR, perm='elradfmwMT')

    # Instantiate FTP handler class
    handler = FTPHandler
    handler.authorizer = authorizer

    # Instantiate FTP server class and listen on 127.0.0.1:2121
    address = ('127.0.0.1', 2121)
    server = FTPServer(address, handler)

    print(f"Starting FTP server on {address[0]}:{address[1]} with user 'user' and pass 'pass'...")
    print(f"Serving directory: {TEST_DIR}")
    
    # set a limit for connections
    server.max_cons = 256
    server.max_cons_per_ip = 5

    # start ftp server
    server.serve_forever()

if __name__ == '__main__':
    main()
