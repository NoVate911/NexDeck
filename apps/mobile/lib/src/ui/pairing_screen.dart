import 'package:material_ui/material_ui.dart';

import '../models/protocol_models.dart';
import '../state/agent_controller.dart';

/// Screen for importing a host pairing QR payload or entering host/PIN manually.
class PairingScreen extends StatefulWidget {
  /// Creates a [PairingScreen].
  const PairingScreen({required this.controller, super.key});

  /// Controller handling `/health`, `/pair`, and WSS connection.
  final AgentController controller;

  @override
  State<PairingScreen> createState() => _PairingScreenState();
}

class _PairingScreenState extends State<PairingScreen> {
  final TextEditingController _qrController = TextEditingController();
  final TextEditingController _hostController = TextEditingController(
    text: '127.0.0.1',
  );
  final TextEditingController _portController = TextEditingController(
    text: '9443',
  );
  final TextEditingController _fingerprintController = TextEditingController();
  final TextEditingController _pinController = TextEditingController();
  final TextEditingController _deviceNameController = TextEditingController(
    text: 'ExamplePhone',
  );

  String? _localValidationError;

  @override
  void dispose() {
    _qrController.dispose();
    _hostController.dispose();
    _portController.dispose();
    _fingerprintController.dispose();
    _pinController.dispose();
    _deviceNameController.dispose();
    super.dispose();
  }

  void _applyQrJson() {
    try {
      final parsed = PairingQrPayload.fromJsonString(_qrController.text.trim());
      setState(() {
        _localValidationError = null;
        _hostController.text = parsed.host;
        _portController.text = parsed.port.toString();
        _fingerprintController.text = parsed.fingerprintSha256;
        _pinController.text = parsed.pin;
      });
    } on FormatException catch (err) {
      setState(() {
        _localValidationError = 'Неверный формат QR JSON: ${err.message}';
      });
    }
  }

  Future<void> _submitPairing() async {
    final port = int.tryParse(_portController.text.trim());
    if (port == null) {
      setState(() {
        _localValidationError = 'Укажите корректный числовой порт TCP.';
      });
      return;
    }
    setState(() {
      _localValidationError = null;
    });
    await widget.controller.pairAndConnect(
      host: _hostController.text,
      port: port,
      fingerprintSha256: _fingerprintController.text,
      pin: _pinController.text,
      deviceName: _deviceNameController.text,
    );
  }

  @override
  Widget build(BuildContext context) {
    final controller = widget.controller;
    final theme = Theme.of(context);
    final displayError = _localValidationError ?? controller.errorMessage;

    return Scaffold(
      appBar: AppBar(title: const Text('NexDeck — Сопряжение с ПК')),
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 560),
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(20),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Card(
                    child: Padding(
                      padding: const EdgeInsets.all(16),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          Text(
                            'Быстрый импорт QR JSON',
                            style: theme.textTheme.titleMedium,
                          ),
                          const SizedBox(height: 8),
                          Text(
                            'Вставьте JSON-текст, который выводит команда nexdeck-agent --daemon --pair:',
                            style: theme.textTheme.bodySmall,
                          ),
                          const SizedBox(height: 12),
                          TextField(
                            key: const Key('qr_json_field'),
                            controller: _qrController,
                            maxLines: 3,
                            decoration: const InputDecoration(
                              border: OutlineInputBorder(),
                              hintText: '{"protocol_version":1,"host":"127.0.0.1","port":9443,...}',
                            ),
                          ),
                          const SizedBox(height: 12),
                          Align(
                            alignment: Alignment.centerRight,
                            child: OutlinedButton.icon(
                              key: const Key('apply_qr_button'),
                              onPressed: controller.isBusy
                                  ? null
                                  : _applyQrJson,
                              icon: const Icon(Icons.qr_code_2),
                              label: const Text('Заполнить поля из QR JSON'),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                  const SizedBox(height: 16),
                  Card(
                    child: Padding(
                      padding: const EdgeInsets.all(16),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          Text(
                            'Параметры хоста и TLS-пиннинг',
                            style: theme.textTheme.titleMedium,
                          ),
                          const SizedBox(height: 14),
                          Row(
                            children: [
                              Expanded(
                                flex: 3,
                                child: TextField(
                                  key: const Key('host_field'),
                                  controller: _hostController,
                                  decoration: const InputDecoration(
                                    labelText: 'Адрес ПК',
                                    hintText: '127.0.0.1',
                                    border: OutlineInputBorder(),
                                  ),
                                ),
                              ),
                              const SizedBox(width: 12),
                              Expanded(
                                flex: 1,
                                child: TextField(
                                  key: const Key('port_field'),
                                  controller: _portController,
                                  keyboardType: TextInputType.number,
                                  decoration: const InputDecoration(
                                    labelText: 'Порт',
                                    hintText: '9443',
                                    border: OutlineInputBorder(),
                                  ),
                                ),
                              ),
                            ],
                          ),
                          const SizedBox(height: 12),
                          TextField(
                            key: const Key('fingerprint_field'),
                            controller: _fingerprintController,
                            decoration: const InputDecoration(
                              labelText:
                                  'SHA-256 отпечаток сертификата (64 hex)',
                              border: OutlineInputBorder(),
                            ),
                          ),
                          const SizedBox(height: 12),
                          Row(
                            children: [
                              Expanded(
                                child: TextField(
                                  key: const Key('pin_field'),
                                  controller: _pinController,
                                  keyboardType: TextInputType.number,
                                  decoration: const InputDecoration(
                                    labelText: 'PIN (6 цифр)',
                                    border: OutlineInputBorder(),
                                  ),
                                ),
                              ),
                              const SizedBox(width: 12),
                              Expanded(
                                child: TextField(
                                  key: const Key('device_name_field'),
                                  controller: _deviceNameController,
                                  decoration: const InputDecoration(
                                    labelText: 'Имя устройства',
                                    hintText: 'ExamplePhone',
                                    border: OutlineInputBorder(),
                                  ),
                                ),
                              ),
                            ],
                          ),
                          if (displayError != null) ...[
                            const SizedBox(height: 14),
                            Container(
                              padding: const EdgeInsets.all(12),
                              decoration: BoxDecoration(
                                color: theme.colorScheme.errorContainer,
                                borderRadius: BorderRadius.circular(8),
                              ),
                              child: Text(
                                displayError,
                                style: TextStyle(
                                  color: theme.colorScheme.onErrorContainer,
                                ),
                              ),
                            ),
                          ],
                          if (controller.statusMessage != null) ...[
                            const SizedBox(height: 12),
                            Text(
                              controller.statusMessage!,
                              style: theme.textTheme.bodySmall,
                            ),
                          ],
                          const SizedBox(height: 16),
                          FilledButton.icon(
                            key: const Key('pair_submit_button'),
                            onPressed: controller.isBusy
                                ? null
                                : _submitPairing,
                            icon: controller.isBusy
                                ? const SizedBox(
                                    width: 16,
                                    height: 16,
                                    child: CircularProgressIndicator(
                                      strokeWidth: 2,
                                    ),
                                  )
                                : const Icon(Icons.link),
                            label: const Text('Сопрячь и подключиться'),
                          ),
                        ],
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
