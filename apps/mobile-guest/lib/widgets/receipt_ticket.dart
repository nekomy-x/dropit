/// «Neon ticket» — перфорированный чек (макет: `1. Receipt — Neon ticket`).
///
/// Request receipt with the payment state returned by the backend.
library;

import 'package:flutter/material.dart';

import '../data/models.dart';
import '../theme/tokens.dart';

class ReceiptTicket extends StatelessWidget {
  const ReceiptTicket({super.key, required this.request});

  final GuestRequest request;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: DropTokens.surface,
        borderRadius: BorderRadius.circular(DropTokens.rSm),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // ── Заголовок ──
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 16, 16, 12),
            child: Row(
              children: [
                const Text('REQUEST RECEIPT', style: DropType.labelTight),
                const Spacer(),
                Text(request.priceLabel,
                    style: DropType.caption.copyWith(fontSize: 13)),
                const SizedBox(width: 10),
                Container(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
                  decoration: BoxDecoration(
                    color: DropTokens.tint1A,
                    borderRadius: BorderRadius.circular(999),
                  ),
                  child: Row(
                    children: [
                      Text(
                        request.paymentLabel.toUpperCase(),
                        style: DropType.labelTight.copyWith(
                          color: DropTokens.accent,
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),

          // ── Перфорация ──
          SizedBox(
            height: 12,
            child: Row(
              children: [
                // Левая выемка (цвет фона экрана).
                Transform.translate(
                  offset: const Offset(-6, 0),
                  child: Container(
                    width: 12,
                    height: 12,
                    decoration: const BoxDecoration(
                      color: DropTokens.bg,
                      shape: BoxShape.circle,
                    ),
                  ),
                ),
                Expanded(
                  child: Row(
                    mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                    children: [
                      for (var i = 0; i < 22; i++)
                        Container(
                          width: 4,
                          height: 1,
                          color: const Color(0x26FFFFFF),
                        ),
                    ],
                  ),
                ),
                // Правая выемка.
                Transform.translate(
                  offset: const Offset(6, 0),
                  child: Container(
                    width: 12,
                    height: 12,
                    decoration: const BoxDecoration(
                      color: DropTokens.bg,
                      shape: BoxShape.circle,
                    ),
                  ),
                ),
              ],
            ),
          ),

          // ── Детали ──
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
            child: Column(
              children: [
                _TicketRow(
                  icon: Icons.schedule,
                  label: 'Requested at',
                  value: 'Tonight · ${request.timeLabel}',
                ),
                _TicketRow(
                  icon: Icons.place_outlined,
                  label: 'Room',
                  value: request.room,
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _TicketRow extends StatelessWidget {
  const _TicketRow({
    required this.icon,
    required this.label,
    required this.value,
  });

  final IconData icon;
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 7),
      child: Row(
        children: [
          Icon(icon, size: 13, color: DropTokens.muted),
          const SizedBox(width: 8),
          Text(label, style: DropType.caption),
          const Spacer(),
          Text(
            value,
            style: DropType.caption.copyWith(
              fontWeight: FontWeight.w600,
              color: DropTokens.text,
            ),
          ),
        ],
      ),
    );
  }
}
