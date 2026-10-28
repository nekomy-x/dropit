/// Guest · 02 · Scan QR + 04c · Manual entry.
///
/// Макет pen.dev: полноэкранная камера, затемнение, «рамочка» сканера,
/// инструменты (вспышка/галерея), ручной ввод кода комнаты (шторка).
library;

import 'package:flutter/material.dart';

import '../data/models.dart';
import '../theme/tokens.dart';
import '../widgets/ui.dart';

class ScanScreen extends StatefulWidget {
  const ScanScreen({
    super.key,
    required this.onClose,
    required this.onJoined,
    this.onCode,
  });

  final VoidCallback onClose;
  final ValueChanged<Room> onJoined;
  final ValueChanged<String>? onCode;

  @override
  State<ScanScreen> createState() => _ScanScreenState();
}

class _ScanScreenState extends State<ScanScreen>
    with SingleTickerProviderStateMixin {
  bool _flash = false;
  late final AnimationController _scanLine;

  @override
  void initState() {
    super.initState();
    _scanLine = AnimationController(
      vsync: this,
      duration: const Duration(seconds: 2),
    )..repeat(reverse: true);
  }

  @override
  void dispose() {
    _scanLine.dispose();
    super.dispose();
  }

  void _openManualEntry() {
    showModalBottomSheet(
      context: context,
      backgroundColor: DropTokens.sheet,
      isScrollControlled: true,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(28)),
      ),
      builder: (_) => const _RoomCodeSheet(),
    ).then((code) {
      if (code is String && code.trim().isNotEmpty) {
        if (widget.onCode != null) {
          widget.onCode!(code.trim());
        } else {
          widget.onJoined(_roomForCode(code.trim()));
        }
      }
    });
  }

  Room _roomForCode(String code) {
    // Демо: любой непустой код ведёт в ближайшую живую комнату.
    return const Room(
      id: 'kater-blau',
      name: 'Kater Blau',
      session: 'Main floor',
      dj: 'DJ Markus',
      genre: 'House',
      distance: '200 m',
      image: 'assets/images/venue_kater_blau.jpg',
      liveCount: 12,
    );
  }

  @override
  Widget build(BuildContext context) {
    return DropScaffold(
      body: Stack(
        fit: StackFit.expand,
        children: [
          // Камера (снимок сцены из макета).
          Image.asset('assets/images/camera_scan.jpg', fit: BoxFit.cover),
          // Затемнение.
          const DecoratedBox(
            decoration: BoxDecoration(
              gradient: LinearGradient(
                begin: Alignment.topCenter,
                end: Alignment.bottomCenter,
                colors: [
                  Color(0xEF000000),
                  Color(0x99000000),
                  Color(0x00000000),
                  Color(0x22000000),
                  Color(0xEE000000),
                  Color(0xEE000000),
                ],
                stops: [0.0, 0.2, 0.42, 0.62, 0.85, 1.0],
              ),
            ),
          ),
          SafeArea(
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const SizedBox(height: 24),

                  // ── Навигация сканера ──
                  SizedBox(
                    height: 44,
                    child: Row(
                      mainAxisAlignment: MainAxisAlignment.spaceBetween,
                      children: [
                        const BrandLockup(markColor: DropTokens.text),
                        GhostCircleButton(
                          icon: Icons.close,
                          size: 44,
                          fill: const Color(0x80121212),
                          bordered: false,
                          onPressed: widget.onClose,
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(height: 38),

                  // ── Вводная часть ──
                  const Text('Scan in. Tune in.', style: DropType.screenTitle),
                  const SizedBox(height: 8),
                  Text(
                    'Point your camera at the club’s QR code.',
                    style: DropType.bodySm.copyWith(
                      fontSize: 13,
                      height: 1.5,
                      color: DropTokens.scanHint,
                    ),
                  ),

                  // ── Рамка сканера (масштабируется на маленьких экранах) ──
                  Expanded(
                    child: Center(
                      child: FittedBox(
                        fit: BoxFit.scaleDown,
                        child: SizedBox(
                          width: 310,
                          height: 300,
                          child: Stack(
                            children: [
                              Container(
                                decoration: BoxDecoration(
                                  borderRadius: BorderRadius.circular(32),
                                  border: Border.all(
                                    color: const Color(0xB3FFFFFF),
                                    width: 2,
                                  ),
                                ),
                              ),
                              // Бегущая линия сканирования.
                              AnimatedBuilder(
                                animation: _scanLine,
                                builder: (context, _) {
                                  return Align(
                                    alignment: Alignment(
                                        0, -0.85 + _scanLine.value * 1.7),
                                    child: Container(
                                      height: 2,
                                      margin: const EdgeInsets.symmetric(
                                          horizontal: 12),
                                      decoration: BoxDecoration(
                                        gradient: LinearGradient(
                                          colors: [
                                            DropTokens.accent
                                                .withValues(alpha: 0),
                                            DropTokens.accent,
                                            DropTokens.accent
                                                .withValues(alpha: 0),
                                          ],
                                        ),
                                      ),
                                    ),
                                  );
                                },
                              ),
                            ],
                          ),
                        ),
                      ),
                    ),
                  ),

                  // ── Подсказка видоискателя ──
                  Center(
                    child: Text(
                      'Scanning automatically',
                      style: DropType.bodySm.copyWith(
                        fontSize: 14,
                        fontWeight: FontWeight.w500,
                        color: DropTokens.text,
                      ),
                    ),
                  ),
                  const SizedBox(height: 10),
                  Center(
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        const Icon(Icons.door_front_door_outlined,
                            size: 13, color: DropTokens.muted),
                        const SizedBox(width: 6),
                        Text(
                          'Just hold steady. No photo needed.',
                          style: DropType.caption
                              .copyWith(color: DropTokens.scanHintDim),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(height: 30),

                  // ── Инструменты камеры ──
                  Row(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      _CameraTool(
                        icon: _flash ? Icons.flash_on : Icons.flash_off,
                        label: _flash ? 'Flash on' : 'Flash',
                        onTap: () => setState(() => _flash = !_flash),
                      ),
                      const SizedBox(width: 32),
                      _CameraTool(
                        icon: Icons.photo_library_outlined,
                        label: 'From photos',
                        onTap: () {
                          if (widget.onCode != null) {
                            widget.onCode!('gallery');
                          } else {
                            widget.onJoined(_roomForCode('gallery'));
                          }
                        },
                      ),
                    ],
                  ),
                  const SizedBox(height: 44),

                  // ── Ручной ввод ──
                  const Text(
                    'Can’t scan the code?',
                    style: DropType.caption,
                  ),
                  const SizedBox(height: 10),
                  InkWell(
                    onTap: _openManualEntry,
                    borderRadius: BorderRadius.circular(999),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(vertical: 8),
                      child: Row(
                        children: [
                          const Icon(Icons.keyboard_outlined,
                              size: 18, color: DropTokens.scanHintDim),
                          const SizedBox(width: 10),
                          Text(
                            'Type room code manually',
                            style: DropType.bodySm.copyWith(
                              fontSize: 13,
                              fontWeight: FontWeight.w500,
                              color: DropTokens.scanHint,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
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

class _CameraTool extends StatelessWidget {
  const _CameraTool({
    required this.icon,
    required this.label,
    required this.onTap,
  });

  final IconData icon;
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 80,
      child: Column(
        children: [
          GhostCircleButton(icon: icon, onPressed: onTap),
          const SizedBox(height: 8),
          Text(
            label,
            style: DropType.caption.copyWith(color: DropTokens.text),
          ),
        ],
      ),
    );
  }
}

/// Guest · 04c · Room code bottom sheet.
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

  void _submit() {
    final code = _controller.text.trim();
    if (code.isEmpty) return;
    Navigator.of(context).pop(code);
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.only(
        left: 44,
        right: 44,
        bottom: MediaQuery.of(context).viewInsets.bottom + 24,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const SizedBox(height: 12),
          Center(
            child: Container(
              width: 36,
              height: 4,
              decoration: BoxDecoration(
                color: DropTokens.handle,
                borderRadius: BorderRadius.circular(2),
              ),
            ),
          ),
          const SizedBox(height: 20),
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              const Text('Enter room code', style: DropType.sheetTitle),
              IconButton(
                onPressed: () => Navigator.of(context).pop(),
                icon:
                    const Icon(Icons.close, size: 22, color: DropTokens.muted),
              ),
            ],
          ),
          const SizedBox(height: 8),
          Text(
            'Enter the code displayed next to the club’s QR.',
            style: DropType.caption.copyWith(height: 1.5),
          ),
          const SizedBox(height: 20),
          const FieldLabel('Room code'),
          DropField(
            controller: _controller,
            hint: 'Enter code',
            prefixIcon: Icons.tag,
            height: 48,
            onSubmitted: (_) => _submit(),
          ),
          const SizedBox(height: 20),
          PrimaryButton(
            label: 'Join room',
            onPressed: _submit,
          ),
        ],
      ),
    );
  }
}
