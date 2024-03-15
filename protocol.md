# ROME computer communication protocol

ROME (device) communicates with computer over arduino's virtual serial port.
Messages sent from computer to device and vice versa consist of ASCII characters.
Each message ends with a single new line character (aka `\n`).

Message sent from computer to device is named *command*.
Device responds to every command using single *response* message.

Device may also send information messages, which are mostly used for debugging and can be ignored by the computer.

## Message types

### Information messages

Information messages contain (usually) human-readable information about device status and/or progress of command execution.
Device may send any number of information messages at any point of communication.

Information message starts with `#` character.
For example, the device usually sends the following message after initialization:
```
# Started
```

### Errors

When device cannot parse received command, instead of a valid response it sends an error message.
Error message consists of `!` character followed by error code.

### Commands

#### Device version request

Requests information about device and firmware version:

```
V
```

Response starts with `V` character followed by version string:

```
VROME-0.1.0
```

#### Ping command

Ping command consists of `P` character followed by arbitrary sequence of other characters:

```
P002030210230213
```

Device should respond with `p` (lowercase) character followed by the same sequence of characters:

```
p002030210230213
```

This command is used to:
- identify ROME device.
  Change of first character allows to differentiate ROME device from some random devices acting as echo.
- ensure that device is alive/functional.
- synchronise inbound and outbound data streams.
  There may be some trash remaining in receive buffer of computer's serial port when communication with device starts.
  Sending ping command with unique sequence of characters makes it possible to skip the trash data until ping response with the same unique character sequence.

#### Self-test

The following command orders device to perform a self-test:

```
T
```

The device will try to write some data to its memory and verify memory content.
In case of success the following message will be returned:

```
TOK
```

In case of test failure, the response will be different:

```
TFAIL
```

The device will also send some information messages reporting test progress and some additional information that may be useful to debug hardware issues in case of failure.

#### Enabling external access

"External access" means that device's memory is accessible using a parallel ROM/RAM interface.
In default state as well as after execution of any commands demanding memory access (read, write, test commands), the ROM/RAM interface is not active.

The following command enables external access to device's memory:

```
E
```

Device will reply with the following response:

```
EOK
```

#### Writing data to device

Command that writes data to device memory consists of `W` character, followed by address of first byte to write as a 4-digit hexadecimal number, followed by data bytes represented as 2-digit hexadecimal numbers each.
No spaces between message parts is allowed.

For example, to write 4 bytes of data starting from address `0000`, the following command may be sent:

```
W0000DEADC0DE
```

Response consists of `W` character followed by addresses of first written byte, followed by next address after the last written byte.
For example, the following response will be sent after execution of previous command:

```
W00000004
```

#### Reading data from device

Read command consists of `R` character followed by address of first readable byte as 4-digit hexadecimal number, followed by number of bytes to read as 2-digit hexadecimal number.
For exampl, the following command reads 4 bytes starting with address `0000`:

```
R000004
```

The response consists of `R` character followed by read bytes, each represented as a 2-digit hexadecimal number:

```
RDEADC0DE
```

Note that maximal size of a readable chunk is limited by 255 bytes.
If more bytes should be read, the operation should be split into multiple commands.
