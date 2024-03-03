#include <SPI.h>

#define SERIAL_BAUD_RATE 250000

#define PIN_ADDRESS_LATCH 19
#define PIN_INTERNAL_ADDRESS_ENABLE_INV 18

#define PIN_INTERNAL_READ_INV 2
#define PIN_INTERNAL_WRITE_INV 3
#define PIN_EXTERNAL_CONTROL_INV 8

#define RAM_SIZE 0x10000
#define ADDRESS_MAX 0xFFFF

namespace data_bus {
  inline void setup_data_write() {
    DDRC = DDRC | 0x0F;
    DDRD = DDRD | 0xF0;
  }

  inline void data_write(uint8_t value) {
    PORTC = PORTC & 0xF0 | value & 0x0F;
    PORTD = PORTD & 0x0F | value & 0xF0;
  }

  inline void setup_data_read() {
    DDRC = DDRC & 0xF0; // set lower 4 bits to 0 (input)
    DDRD = DDRD & 0x0F; // set upper 4 bits to 0 (input)
    data_write(0xFF);
  }

  inline uint8_t data_read() {
    return (PINC & 0x0F) | (PIND & 0xF0);
  }
}

namespace state {
  enum State {
    NotInitialized,
    Writing,
    Reading,
    ExternalControl,
  } current = NotInitialized;

  void setup_internal_control() {
    digitalWrite(PIN_EXTERNAL_CONTROL_INV, 1);

    digitalWrite(PIN_INTERNAL_ADDRESS_ENABLE_INV, 0);

    pinMode(PIN_INTERNAL_READ_INV, OUTPUT);
    digitalWrite(PIN_INTERNAL_READ_INV, 1);
    pinMode(PIN_INTERNAL_WRITE_INV, OUTPUT);
    digitalWrite(PIN_INTERNAL_WRITE_INV, 1);
  }

  void setup_external_control() {
    data_bus::setup_data_read();

    pinMode(PIN_INTERNAL_READ_INV, INPUT_PULLUP);
    pinMode(PIN_INTERNAL_WRITE_INV, INPUT_PULLUP);
    digitalWrite(PIN_INTERNAL_ADDRESS_ENABLE_INV, 1);

    digitalWrite(PIN_EXTERNAL_CONTROL_INV, 0);
  }

  void setup_read() {
    data_bus::setup_data_read();
  }

  void setup_write() {
    data_bus::setup_data_write();
  }

  void setup_state(State state) {
    switch (state) {
    case ExternalControl:
      setup_external_control();
      break;
    case Reading:
      setup_internal_control();
      setup_read();
      break;
    case Writing:
      setup_internal_control();
      setup_write();
      break;
    }

    current = state;
  }

  inline void ensure_state(State state) {
    if (current == state) {
      return;
    }

    setup_state(state);
  }
}

inline void send_address(uint16_t address) {
  SPI.transfer16(address);
  digitalWrite(PIN_ADDRESS_LATCH, 1);
  digitalWrite(PIN_ADDRESS_LATCH, 0);
}

inline uint8_t read_address(uint16_t address) {
  send_address(address);
  digitalWrite(PIN_INTERNAL_READ_INV, 0);
  uint16_t result = data_bus::data_read();
  digitalWrite(PIN_INTERNAL_READ_INV, 1);
  return result;
}

inline uint8_t write_address(uint16_t address, uint8_t value) {
  #if 0
    send_address(address);
    data_bus::data_write(value);
  #else
    union { uint16_t val; struct { uint8_t lsb; uint8_t msb; }; } a;
    a.val = address;
    SPDR = a.msb;
    PORTC = PORTC & 0xF0 | value & 0x0F;
    while (!(SPSR & _BV(SPIF))) ;
    SPDR = a.lsb;
    PORTD = PORTD & 0x0F | value & 0xF0;
    while (!(SPSR & _BV(SPIF))) ;
    digitalWrite(PIN_ADDRESS_LATCH, 1);
    digitalWrite(PIN_ADDRESS_LATCH, 0);
  #endif
  digitalWrite(PIN_INTERNAL_WRITE_INV, 0);
  digitalWrite(PIN_INTERNAL_WRITE_INV, 1);
}

inline char hex_digit(uint8_t value) {
  if (value < 10) {
    return '0' + value;
  }

  return 'A' - 10 + value;
}

void print_hex(uint8_t value) {
  Serial.write(hex_digit(value >> 4));
  Serial.write(hex_digit(value & 0xF));
}

void print_hex(uint16_t value) {
  print_hex(uint8_t(value >> 8));
  print_hex(uint8_t(value & 0xFF));
}

namespace command_input {
  inline void read_char(uint8_t& res) {
    while (!Serial.available()) ;

    int x = Serial.read();

    if (x < 0) {
      exit(1);
    }

    res = x;
  }

  inline void skip_line() {
    uint8_t ch;
    do {
      read_char(ch);
    } while (ch != '\n');
  }

  inline int read_hex_digit(uint8_t& res) {
    read_char(res);

    uint8_t a = res - '0';
    if (a < 10) {
      res = a;
      return 0;
    }

    uint8_t b = res - 'A';

    if (b < 6) {
      res = 10 + b;
      return 0;
    }

    return -1;
  }

  inline int read_hex_byte(uint8_t& res) {
    if (read_hex_digit(res) < 0) {
      return -1;
    }
    uint8_t buf;
    if (read_hex_digit(buf) < 0) {
      return -1;
    }
    res <<= 4;
    res |= buf;
    return 0;
  }

  inline int read_hex_word(uint16_t& res) {
    union { uint16_t val; struct { uint8_t lsb; uint8_t msb; }; } x;
    if (read_hex_byte(x.msb) < 0) {
      return -1;
    }
    if (read_hex_byte(x.lsb) < 0) {
      return -1;
    }
    res = x.val;
    return 0;
  }
}

struct test_result_s {
  bool has_errors;
  uint8_t error_data_mask;
};

bool run_test_0(uint8_t mask, struct test_result_s& result) {
  bool has_errors = false;

  state::ensure_state(state::Writing);
  uint16_t a = 0;
  do {
    write_address(a, mask ^ a ^ (a >> 8));
    if (a == ADDRESS_MAX) break;
    ++a;
  } while (true);
  state::ensure_state(state::Reading);

  a = 0;
  do {
    uint8_t val = read_address(a);
    uint8_t expected = mask ^ a ^ (a >> 8);

    if (val != expected) {
      result.has_errors = true;
      result.error_data_mask |= (val ^ expected);
    }
    if (a == ADDRESS_MAX) break;
    ++a;
  } while (true);

  return has_errors;
}

void run_test() {
  test_result_s result;
  result.error_data_mask = 0;
  result.has_errors = false;

  Serial.write("# Testing pattern 1...\n");
  run_test_0(0, result);

  Serial.write("# Testing pattern 2...\n");
  run_test_0(0xFF, result);

  if (result.has_errors) {
    Serial.write("# Data errors mask: ");
    print_hex(result.error_data_mask);
    Serial.write("\n");
    Serial.write("TFAIL\n");
  } else {
    Serial.write("TOK\n");
  }
}

inline void run_write_bytes(uint16_t start_address) {
  state::ensure_state(state::Writing);

  uint16_t current_address = start_address;
  while (true) {
    uint8_t delimiter;
    command_input::read_char(delimiter);
    switch (delimiter) {
    case ' ':
      break;
    case '\n':
      Serial.write("W");
      print_hex(start_address);
      Serial.write(' ');
      print_hex(current_address);
      Serial.write('\n');
      return;
    default:
      Serial.write("!BADSYNTAX DATA ");
      Serial.write(delimiter);
      Serial.write("\n");
      command_input::skip_line();
      return;
    }
    uint8_t byte;
    if (command_input::read_hex_byte(byte) < 0) {
      Serial.write("!BADARG DATA ");
      print_hex(current_address);
      Serial.write("\n");
      command_input::skip_line();
      return;
    }

    write_address(current_address, byte);
    current_address++;
  };
}

void setup() {
  // SPI setup
  pinMode(PIN_SPI_SCK, OUTPUT);
  pinMode(PIN_SPI_MOSI, OUTPUT);
  SPI.setBitOrder(MSBFIRST);
  SPI.begin();

  // Address bus control lines
  pinMode(PIN_ADDRESS_LATCH, OUTPUT);
  digitalWrite(PIN_ADDRESS_LATCH, 0);
  pinMode(PIN_INTERNAL_ADDRESS_ENABLE_INV, OUTPUT);
  digitalWrite(PIN_INTERNAL_ADDRESS_ENABLE_INV, 1);

  pinMode(PIN_EXTERNAL_CONTROL_INV, OUTPUT);

  // Setup state-dependent lines in internally-controlled, write-ready condition
  state::ensure_state(state::Writing);

  // Serial communication
  Serial.begin(SERIAL_BAUD_RATE);

  Serial.println("# Started");
}

void loop() {
  uint8_t cmd;
  command_input::read_char(cmd);
  switch (cmd) {
    case 'W': // W - write bytes
      uint16_t start_address;
      if (command_input::read_hex_word(start_address) < 0) {
        Serial.write("!BADARG ADDRESS\n");
        command_input::skip_line();
        return;
      }
      run_write_bytes(start_address);
      break;
    case 'R': // R - read bytes
      uint16_t address;
      uint8_t size;
      if (command_input::read_hex_word(address) < 0) {
        Serial.write("!BADARG ADDRESS\n");
        command_input::skip_line();
        return;
      }
      if (command_input::read_hex_byte(size) < 0) {
        Serial.write("!BADARG SIZE\n");
        command_input::skip_line();
        return;
      }
      command_input::skip_line();
      Serial.write('R');
      state::ensure_state(state::Reading);
      while (size--) {
        Serial.write(' ');
        uint8_t data = read_address(address);

        print_hex(data);
        ++address;
      }
      Serial.write('\n');
      break;
    case 'E': // E - enable external access
      state::ensure_state(state::ExternalControl);
      Serial.write("EOK\n");
      break;
    case 'T': // T - run self-test
      command_input::skip_line();
      run_test();
      break;
    case 'V': // V - show version
      Serial.write("VROME-0.0.1a\n");
      command_input::skip_line();
      break;
    default:
      Serial.write("!BADCMD ");
      Serial.write(cmd);
      Serial.write('\n');
      command_input::skip_line();
      break;
  }
}













