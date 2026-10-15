import 'package:flutter/material.dart';

import '../api/api_client.dart';
import '../theme/tokens.dart';
import '../widgets/app_button.dart';
import '../widgets/brand_wave_logo.dart';
import '../widgets/sheet.dart';

/// Guest · 02 · Scan QR (+ Guest · 04c · Manual entry)
///
/// Макет: кадры «Guest · 02 · Scan QR» и «Guest · 04c · Manual entry».
/// Рамка сканирования 310×300 с рамкой акцента, стеклянные кнопки-таблетки,
/// шторка ручного ввода кода с полем 48 и кнопкой «Join room».
///
/// ЗАГЛУШКА (правило 5): сам поток камеры подключается вместе с
/// mobile_scanner под устройство (B5). Здесь — экран и ручной ввод кода,
/// который реально вызывает `POST /v1/sessions/join`.
class ScanScreen extends StatefulWidget {
  const ScanScreen({
    super.key,
    required this.api,
    required this.onJoined,
    this.onClose,
  });

  final ApiClient api;
  final void Function(JoinResponse) onJoined;
  final VoidCallback? onClose;

  @override
  State<ScanScreen> createState() => _ScanScreenState();
}

class _ScanScreenState extends State<ScanScreen> {
  bool _busy = false;
  String? _error;

  Future<void> _joinWithCode(String rawCode) async {
    final token = rawCode.trim();
    if (token.isEmpty) {
      setState(() => _error = 'Enter the code from the club QR');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final session = await widget.api.joinByQr(token);
      if (mounted) Navigator.of(context).maybePop();
      widget.onJoined(session);
    } on ApiException catch (e) {
      setState(() {
        _error = switch (e.envelope.code) {
          'not_found' => 'QR not found or expired — ask staff to regenerate',
          'session_closed' => 'This session is closed',
          'unauthorized' => 'Device not recognized — restart the app',
          _ => e.envelope.message,
        };
      });
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _openManualEntry() async {
    final code = await showModalBottomSheet<String>(
      context: context,
      isScrollControlled: true,
      backgroundColor: Colors.transparent,
      builder: (_) => const _RoomCodeSheet(),
    );
    if (code != null) await _joinWithCode(code);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: DropTokens.bg,
      body: Stack(
        children: [
          // «Камера»: заглушка вместо потока (B5).
          const Positioned.fill(child: _CameraBackdrop()),
          SafeArea(
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const SizedBox(height: 24),
                  Row(
                    children: [
                      const BrandWaveLogo(),
                      const Spacer(),
                      _GlassCircle(
                        icon: Icons.close,
                        size: 44,
                        onTap: widget.onClose ??
                            () => Navigator.of(context).maybePop(),
                      ),
                    ],
                  ),
                  const SizedBox(height: 42),
                  const Text('Scan in. Tune in.',
                      style: DropType.screenTitle),
                  const SizedBox(height: 10),
                  Text(
                    'Point your camera at the club’s QR code.',
                    style: DropType.bodySm.copyWith(color: DropTokens.muted4),
                  ),
                  if (_error != null) ...[
                    const SizedBox(height: 14),
                    _ErrorNote(message: _error!),
                  ],
                  const Spacer(),
                  Text('Scanning automatically',
                      style: DropType.bodySm.copyWith(
                        fontSize: 14,
                        fontWeight: FontWeight.w500,
                      )),
                  const SizedBox(height: 8),
                  Row(
                    children: [
                      const Icon(Icons.door_front_door_outlined,
                          size: 13, color: DropTokens.hint),
                      const SizedBox(width: 6),
                      Text('Just hold steady. No photo needed.',
                          style: DropType.metaSm
                              .copyWith(color: DropTokens.hint)),
                    ],
                  ),
                  const SizedBox(height: 28),
                  const _CameraTools(),
                  const SizedBox(height: 28),
                  Text('Can’t scan the code?',
                      style: DropType.caption.copyWith(
                        color: DropTokens.muted,
                      )),
                  const SizedBox(height: 8),
                  _EnterCodeButton(onTap: _openManualEntry, busy: _busy),
                  const SizedBox(height: 24),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Тёмный фон «камеры» с затемнением по краям и рамкой сканирования.
class _CameraBackdrop extends StatelessWidget {
  const _CameraBackdrop();

  @override
  Widget build(BuildContext context) {
    return Stack(
      fit: StackFit.expand,
      children: [
        const DecoratedBox(
          decoration: BoxDecoration(
            gradient: LinearGradient(
              begin: Alignment.topLeft,
              end: Alignment.bottomRight,
              colors: [Color(0xFF191D1B), DropTokens.rail],
            ),
          ),
        ),
        DecoratedBox(
          decoration: BoxDecoration(
            gradient: LinearGradient(
              begin: Alignment.topCenter,
              end: Alignment.bottomCenter,
              colors: [
                DropTokens.bg.withValues(alpha: 0.94),
                DropTokens.bg.withValues(alpha: 0.13),
                DropTokens.bg.withValues(alpha: 0.07),
                DropTokens.bg.withValues(alpha: 0.93),
              ],
              stops: const [0, 0.32, 0.58, 1],
            ),
          ),
        ),
        Align(
          alignment: const Alignment(0, -0.22),
          child: Container(
            width: 310,
            height: 300,
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(DropTokens.rSheet),
              border: Border.all(color: DropTokens.accent, width: 3),
            ),
          ),
        ),
      ],
    );
  }
}

/// Стеклянные кнопки «Flash» и «From photos» (таблетки 52 с подписью).
class _CameraTools extends StatelessWidget {
  const _CameraTools();

  @override
  Widget build(BuildContext context) {
    return const Row(
      children: [
        Expanded(child: _ToolPill(icon: Icons.flash_off, label: 'Flash')),
        SizedBox(width: 12),
        Expanded(
            child: _ToolPill(icon: Icons.photo_library_outlined,
                label: 'From photos')),
      ],
    );
  }
}

class _ToolPill extends StatelessWidget {
  const _ToolPill({required this.icon, required this.label});

  final IconData icon;
  final String label;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 52,
      decoration: BoxDecoration(
        color: const Color(0xCC161C19),
        borderRadius: BorderRadius.circular(26),
        border: Border.all(color: const Color(0x30FFFFFF)),
      ),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(icon, size: 18, color: DropTokens.text),
          const SizedBox(width: 9),
          Text(label,
              style: DropType.caption.copyWith(
                fontSize: 12,
                fontWeight: FontWeight.w500,
                color: DropTokens.text,
              )),
        ],
      ),
    );
  }
}

class _EnterCodeButton extends StatelessWidget {
  const _EnterCodeButton({required this.onTap, this.busy = false});

  final VoidCallback onTap;
  final bool busy;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 52,
      width: double.infinity,
      child: Material(
        color: const Color(0xFF1B3025),
        borderRadius: BorderRadius.circular(26),
        child: InkWell(
          borderRadius: BorderRadius.circular(26),
          onTap: busy ? null : onTap,
          child: Container(
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(26),
              border: Border.all(color: DropTokens.accent),
            ),
            alignment: Alignment.center,
            child: busy
                ? const SizedBox(
                    width: 18,
                    height: 18,
                    child: CircularProgressIndicator(
                        strokeWidth: 2, color: DropTokens.accent),
                  )
                : Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      const Icon(Icons.keyboard,
                          size: 18, color: DropTokens.accent),
                      const SizedBox(width: 10),
                      Text('Enter room code',
                          style: DropType.bodySm.copyWith(
                            fontSize: 13,
                            fontWeight: FontWeight.w600,
                            color: DropTokens.accent,
                          )),
                    ],
                  ),
          ),
        ),
      ),
    );
  }
}

class _GlassCircle extends StatelessWidget {
  const _GlassCircle({required this.icon, this.onTap, this.size = 52});

  final IconData icon;
  final VoidCallback? onTap;
  final double size;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: size,
      height: size,
      child: Material(
        color: const Color(0x80121212),
        shape: const CircleBorder(
          side: BorderSide(color: DropTokens.ghostStroke),
        ),
        child: InkWell(
          customBorder: const CircleBorder(),
          onTap: onTap,
          child: Icon(icon, size: size * 0.45, color: DropTokens.text),
        ),
      ),
    );
  }
}

class _ErrorNote extends StatelessWidget {
  const _ErrorNote({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: DropTokens.redSoft,
        borderRadius: BorderRadius.circular(DropTokens.rMd),
        border: Border.all(color: DropTokens.red),
      ),
      child: Row(
        children: [
          const Icon(Icons.error_outline, size: 24, color: DropTokens.red),
          const SizedBox(width: 10),
          Expanded(
            child: Text(
              message,
              style: DropType.caption.copyWith(
                fontSize: 13,
                fontWeight: FontWeight.w600,
                color: DropTokens.red,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Guest · 04c: шторка «Enter room code».
class _RoomCodeSheet extends StatefulWidget {
  const _RoomCodeSheet();

  @override
  State<_RoomCodeSheet> createState() => _RoomCodeSheetState();
}

class _RoomCodeSheetState extends State<_RoomCodeSheet> {
  final _controller = TextEditingController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _submit() => Navigator.of(context).pop(_controller.text);

  @override
  Widget build(BuildContext context) {
    return AppSheet(
      title: 'Enter room code',
      trailing: IconButton(
        icon: const Icon(Icons.close, size: 22, color: DropTokens.muted),
        onPressed: () => Navigator.of(context).pop(),
      ),
      children: [
        Text(
          'Enter the code displayed next to the club’s QR.',
          style: DropType.caption.copyWith(height: 1.5),
        ),
        const SizedBox(height: 20),
        SheetField(
          label: 'Room code',
          placeholder: 'Enter code',
          icon: Icons.tag,
          controller: _controller,
          keyboardType: TextInputType.text,
        ),
        const SizedBox(height: 24),
        AppButton(
          label: 'Join room',
          onPressed: _submit,
        ),
      ],
    );
  }
}
