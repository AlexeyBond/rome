#include <SPI.h>

#define PIN_ADDRESS_LATCH 19
#define PIN_INTERNAL_ADDRESS_ENABLE_INV 18

#define PIN_INTERNAL_READ_INV 2
#define PIN_INTERNAL_WRITE_INV 3

#define RAM_SIZE 0x1000

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
    // TODO: Disable buffers

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

    // TODO: Enable buffers
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

void send_address(uint16_t address) {
  SPI.transfer16(address);
  digitalWrite(PIN_ADDRESS_LATCH, 1);
  digitalWrite(PIN_ADDRESS_LATCH, 0);
}

uint8_t read_address(uint16_t address) {
  send_address(address);
  digitalWrite(PIN_INTERNAL_READ_INV, 0);
  uint16_t result = data_bus::data_read();
  digitalWrite(PIN_INTERNAL_READ_INV, 1);
  return result;
}

uint8_t write_address(uint16_t address, uint8_t value) {
  send_address(address);
  data_bus::data_write(value);
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
  print_hex(uint8_t(value && 0xFF));
}

bool run_test_0(uint8_t mask) {
  bool has_errors = false;

  state::ensure_state(state::Writing);
  for (uint16_t a = 0; a < RAM_SIZE; ++a) {
    write_address(a, mask ^ a ^ (a >> 8));
  }
  state::ensure_state(state::Reading);
  for (uint16_t a = 0; a < RAM_SIZE; ++a) {
    uint8_t val = read_address(a);
    uint8_t expected = mask ^ a ^ (a >> 8);

    if (val != expected) {
      Serial.write("# Error at address ");
      print_hex(a);
      Serial.write(" expected ");
      print_hex(expected);
      Serial.write(" got ");
      print_hex(val);
      Serial.write('\n');
      has_errors = true;
    }
  }

  return has_errors;
}

void run_test() {
  Serial.write("# Testing pattern 1...\n");

  bool has_errors = run_test_0(0);

  Serial.write("# Testing pattern 2...\n");

  has_errors = run_test_0(0xFF) || has_errors;

  if (has_errors) {
    Serial.write("!FAIL\n");
  } else {
    Serial.write("!OK\n");
  }
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

  // Setup state-dependent lines in internally-controlled, write-ready condition
  state::ensure_state(state::Writing);

  // Serial communication
  Serial.begin(2000000);

  Serial.println("# Started");
}

void loop() {
  run_test();
  state::ensure_state(state::ExternalControl);

  delay(1000);
}













